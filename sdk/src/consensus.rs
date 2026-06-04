//! Hybrid SHA256d Proof-of-Work / Proof-of-Stake consensus types for the SDK.
//! These types are imported by `core`, `runtime`, `validator`, and `genesis`.

use serde::{Deserialize, Serialize};

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
    pub initial_difficulty_bits: u32,
    pub difficulty_adjustment_window: u64,
    pub target_block_time_secs: u64,
    pub coinbase_maturity: u64,
    pub initial_block_reward: u64,
    pub halving_interval: u64,
}

impl Default for PowConfig {
    fn default() -> Self {
        Self {
            initial_difficulty_bits: 20,
            difficulty_adjustment_window: 2016,
            target_block_time_secs: 120,
            coinbase_maturity: 100,
            initial_block_reward: 50 * 1_000_000_000,
            halving_interval: 210_000,
        }
    }
}

impl PowConfig {
    pub fn target_window_timespan(&self) -> u64 {
        self.difficulty_adjustment_window * self.target_block_time_secs
    }

    pub fn block_reward_at_height(&self, height: u64) -> u64 {
        let halvings = height / self.halving_interval;
        if halvings >= 64 { return 0; }
        self.initial_block_reward >> halvings
    }
}

// ---------------------------------------------------------------------------
// PoS configuration
// ---------------------------------------------------------------------------

/// Proof-of-Stake configuration embedded in genesis and used by the staker.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PosConfig {
    pub min_stake_lamports: u64,
    pub min_coin_age_secs: u64,
    pub max_coin_age_secs: u64,
    pub annual_reward_rate: f64,
    pub target_block_time_secs: u64,
}

impl Default for PosConfig {
    fn default() -> Self {
        Self {
            min_stake_lamports: 100 * 1_000_000_000,
            min_coin_age_secs: 86_400,
            max_coin_age_secs: 86_400 * 90,
            annual_reward_rate: 0.05,
            target_block_time_secs: 60,
        }
    }
}

impl PosConfig {
    pub fn coin_age(&self, lamports: u64, held_secs: u64) -> u64 {
        let capped = held_secs.min(self.max_coin_age_secs);
        lamports.saturating_mul(capped)
    }

    pub fn stake_weight(&self, lamports: u64, held_secs: u64) -> u64 {
        if lamports < self.min_stake_lamports || held_secs < self.min_coin_age_secs {
            return 0;
        }
        self.coin_age(lamports, held_secs)
    }

    pub fn staking_reward(&self, lamports: u64, held_secs: u64) -> u64 {
        let days = held_secs.min(self.max_coin_age_secs) as f64 / 86_400.0;
        (lamports as f64 * self.annual_reward_rate * days / 365.0) as u64
    }
}

// ---------------------------------------------------------------------------
// Unified hybrid consensus config
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HybridConsensusConfig {
    pub pow: PowConfig,
    pub pos: PosConfig,
    pub coin_name: String,
    pub ticker: String,
    pub max_supply_lamports: u64,
    pub genesis_timestamp: i64,
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
// Block header consensus extension
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockConsensusData {
    pub consensus_type: ConsensusType,
    pub nonce: u64,
    pub difficulty_bits: u32,
    pub stake_modifier: [u8; 32],
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

pub fn retarget_difficulty(
    current_bits: u32,
    actual_window_secs: u64,
    target_window_secs: u64,
    min_bits: u32,
) -> u32 {
    let clamped = actual_window_secs
        .max(target_window_secs / 4)
        .min(target_window_secs * 4);
    let ratio = (clamped * 1000) / target_window_secs;
    if ratio < 900 {
        (current_bits + 1).min(240)
    } else if ratio > 1100 {
        current_bits.saturating_sub(1).max(min_bits)
    } else {
        current_bits
    }
}

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
        assert_eq!(cfg.block_reward_at_height(210_000), cfg.block_reward_at_height(0) / 2);
    }

    #[test]
    fn test_stake_weight_immature() {
        let cfg = PosConfig::default();
        assert_eq!(cfg.stake_weight(1_000 * 1_000_000_000, 3600), 0);
    }

    #[test]
    fn test_hash_meets_difficulty_zeros() {
        assert!(hash_meets_difficulty(&[0u8; 32], 64));
    }

    #[test]
    fn test_hash_fails_difficulty() {
        let mut hash = [0u8; 32];
        hash[0] = 0x80;
        assert!(!hash_meets_difficulty(&hash, 1));
    }
}
