//! Hybrid SHA256d Proof-of-Work / Proof-of-Stake consensus types for the SDK.
//! These types are imported by `core`, `runtime`, `validator`, and `genesis`.

use serde::{Deserialize, Serialize};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Chain-wide consensus parameters
// ---------------------------------------------------------------------------

/// Top-level consensus mode for a block.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum ConsensusType {
    /// Proof-of-Work block produced by SHA256d mining.
    ProofOfWork,
    /// Proof-of-Stake block produced by coin-age minting.
    ProofOfStake,
}

impl std::fmt::Display for ConsensusType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConsensusType::ProofOfWork => write!(f, "PoW"),
            ConsensusType::ProofOfStake => write!(f, "PoS"),
        }
    }
}

// ---------------------------------------------------------------------------
// PoW configuration
// ---------------------------------------------------------------------------

/// Proof-of-Work configuration embedded in genesis and used by the miner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PowConfig {
    /// Number of leading zero bits required in a valid block hash (initial).
    pub initial_difficulty_bits: u32,
    /// Number of blocks between difficulty retargets (Bitcoin-style window).
    pub difficulty_adjustment_window: u64,
    /// Target wall-clock time per PoW block (seconds).
    pub target_block_time_secs: u64,
    /// Blocks before a coinbase reward can be spent.
    pub coinbase_maturity: u64,
    /// Initial block subsidy in base units (lamports).
    pub initial_block_reward: u64,
    /// Number of blocks between reward halvings.
    pub halving_interval: u64,
}

impl Default for PowConfig {
    fn default() -> Self {
        Self {
            initial_difficulty_bits: 20,
            difficulty_adjustment_window: 2016,
            target_block_time_secs: 120,   // 2 minutes
            coinbase_maturity: 100,
            initial_block_reward: 50 * 1_000_000_000, // 50 coins in lamports
            halving_interval: 210_000,
        }
    }
}

impl PowConfig {
    /// Target total timespan for one difficulty window (seconds).
    pub fn target_window_timespan(&self) -> u64 {
        self.difficulty_adjustment_window * self.target_block_time_secs
    }

    /// Compute block reward at a given block height, applying halvings.
    pub fn block_reward_at_height(&self, height: u64) -> u64 {
        let halvings = height / self.halving_interval;
        if halvings >= 64 {
            return 0;
        }
        self.initial_block_reward >> halvings
    }
}

// ---------------------------------------------------------------------------
// PoS configuration
// ---------------------------------------------------------------------------

/// Proof-of-Stake configuration embedded in genesis and used by the staker.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PosConfig {
    /// Minimum coin balance (in base units) required to stake.
    pub min_stake_lamports: u64,
    /// Minimum time coins must be held before they can stake (seconds).
    pub min_coin_age_secs: u64,
    /// Maximum coin age that accumulates toward stake weight (seconds).
    /// Coins held longer than this cap out — prevents hoarding attacks.
    pub max_coin_age_secs: u64,
    /// Annual PoS reward rate expressed as a fraction (e.g. 0.05 = 5%).
    pub annual_reward_rate: f64,
    /// Target wall-clock time per PoS block (seconds).
    pub target_block_time_secs: u64,
}

impl Default for PosConfig {
    fn default() -> Self {
        Self {
            min_stake_lamports: 100 * 1_000_000_000, // 100 coins
            min_coin_age_secs: 86_400,               // 1 day
            max_coin_age_secs: 86_400 * 90,          // 90 days
            annual_reward_rate: 0.05,                // 5%
            target_block_time_secs: 60,              // 1 minute
        }
    }
}

impl PosConfig {
    /// Calculate coin-age for a UTXO (capped at max_coin_age_secs).
    pub fn coin_age(&self, lamports: u64, held_secs: u64) -> u64 {
        let capped = held_secs.min(self.max_coin_age_secs);
        lamports.saturating_mul(capped)
    }

    /// Stake weight — zero if coin or age requirements not met.
    pub fn stake_weight(&self, lamports: u64, held_secs: u64) -> u64 {
        if lamports < self.min_stake_lamports || held_secs < self.min_coin_age_secs {
            return 0;
        }
        self.coin_age(lamports, held_secs)
    }

    /// PoS block reward for consuming a given coin-age.
    pub fn staking_reward(&self, lamports: u64, held_secs: u64) -> u64 {
        let days = held_secs.min(self.max_coin_age_secs) as f64 / 86_400.0;
        (lamports as f64 * self.annual_reward_rate * days / 365.0) as u64
    }
}

// ---------------------------------------------------------------------------
// Unified hybrid consensus config (replaces PohConfig in GenesisConfig)
// ---------------------------------------------------------------------------

/// Combined PoW + PoS configuration stored in genesis and propagated to all nodes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridConsensusConfig {
    pub pow: PowConfig,
    pub pos: PosConfig,
    /// Human-readable coin name.
    pub coin_name: String,
    /// Ticker symbol.
    pub ticker: String,
    /// Maximum total supply in base units (lamports).
    pub max_supply_lamports: u64,
    /// Unix timestamp of the genesis block.
    pub genesis_timestamp: i64,
    /// Arbitrary genesis message embedded in the coinbase.
    pub genesis_message: String,
}

impl Default for HybridConsensusConfig {
    fn default() -> Self {
        Self {
            pow: PowConfig::default(),
            pos: PosConfig::default(),
            coin_name: "HybridChain".to_string(),
            ticker: "HYB".to_string(),
            max_supply_lamports: 21_000_000 * 1_000_000_000,
            genesis_timestamp: 1_748_996_400,
            genesis_message: "HybridChain genesis — SHA256d PoW/PoS hybrid blockchain".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Block header extension for PoW/PoS metadata
// ---------------------------------------------------------------------------

/// Consensus-specific fields appended to every block header.
/// Replaces PoH tick/hash fields in the Solana block header.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockConsensusData {
    /// Whether this block was produced by PoW mining or PoS minting.
    pub consensus_type: ConsensusType,
    /// PoW: winning nonce. PoS: 0.
    pub nonce: u64,
    /// PoW: difficulty target (leading zero bits). PoS: inherited from last PoW block.
    pub difficulty_bits: u32,
    /// PoS: stake modifier derived from previous blocks (prevents grinding).
    /// PoW: all zeros.
    pub stake_modifier: [u8; 32],
    /// PoS: coin-age consumed to mint this block (in coin-seconds).
    /// PoW: 0.
    pub coin_age_consumed: u64,
}

impl Default for BlockConsensusData {
    fn default() -> Self {
        Self {
            consensus_type: ConsensusType::ProofOfWork,
            nonce: 0,
            difficulty_bits: 20,
            stake_modifier: [0u8; 32],
            coin_age_consumed: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Difficulty retargeting
// ---------------------------------------------------------------------------

/// Retarget the PoW difficulty based on how long the last window actually took.
/// Returns new difficulty in leading-zero bits.
/// Clamps adjustment to 4x in either direction (Bitcoin-style).
pub fn retarget_difficulty(
    current_bits: u32,
    actual_window_secs: u64,
    target_window_secs: u64,
    min_bits: u32,
) -> u32 {
    let clamped = actual_window_secs
        .max(target_window_secs / 4)
        .min(target_window_secs * 4);

    // Scale: if actual < target, blocks came fast → raise difficulty (+1 bit)
    //        if actual > target, blocks came slow → lower difficulty (-1 bit)
    let ratio = (clamped * 1000) / target_window_secs;
    if ratio < 900 {
        (current_bits + 1).min(240)
    } else if ratio > 1100 {
        current_bits.saturating_sub(1).max(min_bits)
    } else {
        current_bits
    }
}

/// Check whether a SHA256d hash satisfies the given difficulty (leading zero bits).
pub fn hash_meets_difficulty(hash: &[u8; 32], difficulty_bits: u32) -> bool {
    let full_bytes = (difficulty_bits / 8) as usize;
    let remainder = difficulty_bits % 8;
    for i in 0..full_bytes.min(32) {
        if hash[i] != 0 { return false; }
    }
    if remainder > 0 && full_bytes < 32 {
        let mask = 0xFF_u8 << (8 - remainder);
        if hash[full_bytes] & mask != 0 { return false; }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_reward_halving() {
        let cfg = PowConfig::default();
        let initial = cfg.block_reward_at_height(0);
        let after_first_halving = cfg.block_reward_at_height(210_000);
        assert_eq!(after_first_halving, initial / 2);
    }

    #[test]
    fn test_stake_weight_immature() {
        let cfg = PosConfig::default();
        let weight = cfg.stake_weight(1_000 * 1_000_000_000, 3600); // 1 hour
        assert_eq!(weight, 0); // below min coin age
    }

    #[test]
    fn test_hash_meets_difficulty_zeros() {
        let hash = [0u8; 32];
        assert!(hash_meets_difficulty(&hash, 64));
    }

    #[test]
    fn test_hash_fails_difficulty() {
        let mut hash = [0u8; 32];
        hash[0] = 0x80;
        assert!(!hash_meets_difficulty(&hash, 1));
    }

    #[test]
    fn test_retarget_raises_on_fast_blocks() {
        let cfg = PowConfig::default();
        let target = cfg.target_window_timespan();
        let new_bits = retarget_difficulty(20, target / 2, target, 16);
        assert!(new_bits > 20);
    }

    #[test]
    fn test_retarget_lowers_on_slow_blocks() {
        let cfg = PowConfig::default();
        let target = cfg.target_window_timespan();
        let new_bits = retarget_difficulty(20, target * 2, target, 16);
        assert!(new_bits < 20);
    }
}
