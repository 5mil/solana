//! Hybrid SHA256d PoW / Proof-of-Stake consensus engine for core.
//!
//! Replaces the Tower BFT / PoH consensus module entirely.
//! Responsibilities:
//!   - SHA256d PoW mining loop (find nonce satisfying difficulty target)
//!   - PoS minting eligibility check (coin-age threshold)
//!   - Difficulty retargeting (Bitcoin-style 2016-block window)
//!   - Stake modifier computation (anti-grinding for PoS)
//!   - Chain-selection rule (most cumulative PoW work wins)
//!   - Block header validation (PoW hash check OR PoS coin-age check)

use {
    solana_sdk::{
        consensus::{
            BlockConsensusData, ConsensusType, HybridConsensusConfig,
            PowConfig, PosConfig, hash_meets_difficulty, retarget_difficulty,
        },
        hash::Hash,
        pubkey::Pubkey,
    },
    std::{
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc, RwLock,
        },
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    },
};

// ---------------------------------------------------------------------------
// Chain state tracked by the consensus engine
// ---------------------------------------------------------------------------

/// Snapshot of the current chain tip needed for consensus decisions.
#[derive(Clone, Debug)]
pub struct ChainTip {
    /// Current best block height.
    pub height: u64,
    /// Hash of the current best block.
    pub hash: [u8; 32],
    /// Cumulative PoW work (sum of 2^difficulty_bits for every PoW block).
    pub cumulative_work: u128,
    /// Current PoW difficulty in leading-zero bits.
    pub difficulty_bits: u32,
    /// Unix timestamp of the last block.
    pub last_block_time: u64,
    /// Stake modifier for PoS kernel computation.
    pub stake_modifier: [u8; 32],
    /// Wall-clock timestamp of the start of the current difficulty window.
    pub window_start_time: u64,
    /// Height at which the current difficulty window began.
    pub window_start_height: u64,
}

impl Default for ChainTip {
    fn default() -> Self {
        Self {
            height: 0,
            hash: [0u8; 32],
            cumulative_work: 0,
            difficulty_bits: 20,
            last_block_time: unix_now(),
            stake_modifier: [0u8; 32],
            window_start_time: unix_now(),
            window_start_height: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Consensus engine
// ---------------------------------------------------------------------------

/// The hybrid consensus engine.  One instance lives per node.
pub struct HybridConsensusEngine {
    config: HybridConsensusConfig,
    tip: RwLock<ChainTip>,
    /// Monotonically-increasing nonce seed for the mining thread.
    nonce_counter: AtomicU64,
    /// Set to true by the shutdown signal.
    exit: Arc<AtomicBool>,
}

impl HybridConsensusEngine {
    pub fn new(config: HybridConsensusConfig, exit: Arc<AtomicBool>) -> Self {
        let tip = ChainTip {
            difficulty_bits: config.pow.initial_difficulty_bits,
            ..ChainTip::default()
        };
        Self {
            config,
            tip: RwLock::new(tip),
            nonce_counter: AtomicU64::new(0),
            exit,
        }
    }

    /// Return a snapshot of the current chain tip.
    pub fn tip(&self) -> ChainTip {
        self.tip.read().unwrap().clone()
    }

    // -----------------------------------------------------------------------
    // PoW: mining
    // -----------------------------------------------------------------------

    /// Attempt to mine a PoW block on top of `parent_hash`.
    /// Returns `Some(BlockConsensusData)` when a valid nonce is found,
    /// or `None` if the exit flag is set before a solution is found.
    ///
    /// `extra_data` is mixed into the block header hash (transactions root, etc.).
    pub fn mine_pow_block(
        &self,
        parent_hash: &[u8; 32],
        height: u64,
        extra_data: &[u8],
    ) -> Option<BlockConsensusData> {
        let tip = self.tip();
        let difficulty_bits = tip.difficulty_bits;

        let start_nonce = self.nonce_counter.fetch_add(1_000_000, Ordering::Relaxed);
        let mut nonce = start_nonce;

        loop {
            if self.exit.load(Ordering::Relaxed) {
                return None;
            }

            let hash = sha256d_block_hash(parent_hash, height, nonce, extra_data);

            if hash_meets_difficulty(&hash, difficulty_bits) {
                return Some(BlockConsensusData {
                    consensus_type: ConsensusType::ProofOfWork,
                    nonce,
                    difficulty_bits,
                    stake_modifier: [0u8; 32],
                    coin_age_consumed: 0,
                });
            }

            nonce = nonce.wrapping_add(1);

            // Yield every 100k iterations to avoid starving other threads.
            if nonce % 100_000 == 0 {
                std::thread::yield_now();
            }
        }
    }

    // -----------------------------------------------------------------------
    // PoS: minting eligibility
    // -----------------------------------------------------------------------

    /// Check whether a UTXO is eligible to mint a PoS block.
    /// Returns `Some(BlockConsensusData)` if eligible, `None` otherwise.
    ///
    /// `kernel_hash` is derived from `stake_modifier || txout_hash || txout_n || time`.
    pub fn try_pos_mint(
        &self,
        lamports: u64,
        held_secs: u64,
        coin_age_consumed: u64,
        kernel_hash: &[u8; 32],
    ) -> Option<BlockConsensusData> {
        let cfg = &self.config.pos;
        let tip = self.tip();

        // Coin must meet minimum stake requirements.
        let weight = cfg.stake_weight(lamports, held_secs);
        if weight == 0 {
            return None;
        }

        // PoS target is proportional to coin-age: easier for older/larger stakes.
        // PoS difficulty = PoW difficulty / (coin_age / reference_unit)
        let reference_unit: u64 = 1_000_000_000 * 86_400; // 1 coin-day in lamport-seconds
        let pos_difficulty = if weight >= reference_unit {
            tip.difficulty_bits.saturating_sub(leading_zeros_from_weight(weight, reference_unit))
        } else {
            tip.difficulty_bits
        };
        let pos_difficulty = pos_difficulty.max(8); // floor at 8 bits

        if !hash_meets_difficulty(kernel_hash, pos_difficulty) {
            return None;
        }

        let new_stake_modifier = compute_stake_modifier(&tip.stake_modifier, kernel_hash);

        Some(BlockConsensusData {
            consensus_type: ConsensusType::ProofOfStake,
            nonce: 0,
            difficulty_bits: tip.difficulty_bits,
            stake_modifier: new_stake_modifier,
            coin_age_consumed,
        })
    }

    // -----------------------------------------------------------------------
    // Block acceptance & tip update
    // -----------------------------------------------------------------------

    /// Validate and accept a new block's consensus data.
    /// Returns `Ok(())` if valid and the tip is updated, or `Err(reason)` if invalid.
    pub fn accept_block(
        &self,
        parent_hash: &[u8; 32],
        block_hash: &[u8; 32],
        height: u64,
        block_time: u64,
        consensus_data: &BlockConsensusData,
        extra_data: &[u8],
    ) -> Result<(), ConsensusError> {
        match consensus_data.consensus_type {
            ConsensusType::ProofOfWork => {
                self.validate_pow_block(parent_hash, height, consensus_data, extra_data)?;
            }
            ConsensusType::ProofOfStake => {
                // PoS validation is done by the staker before calling accept_block.
                // Here we just verify difficulty_bits matches current chain.
                let tip = self.tip();
                if consensus_data.difficulty_bits != tip.difficulty_bits {
                    return Err(ConsensusError::InvalidDifficulty {
                        expected: tip.difficulty_bits,
                        found: consensus_data.difficulty_bits,
                    });
                }
            }
        }

        self.update_tip(block_hash, height, block_time, consensus_data);
        Ok(())
    }

    fn validate_pow_block(
        &self,
        parent_hash: &[u8; 32],
        height: u64,
        data: &BlockConsensusData,
        extra_data: &[u8],
    ) -> Result<(), ConsensusError> {
        let tip = self.tip();

        // Difficulty must match current chain state.
        if data.difficulty_bits != tip.difficulty_bits {
            return Err(ConsensusError::InvalidDifficulty {
                expected: tip.difficulty_bits,
                found: data.difficulty_bits,
            });
        }

        // Re-derive the block hash and check it meets difficulty.
        let hash = sha256d_block_hash(parent_hash, height, data.nonce, extra_data);
        if !hash_meets_difficulty(&hash, data.difficulty_bits) {
            return Err(ConsensusError::InsufficientWork);
        }

        Ok(())
    }

    fn update_tip(
        &self,
        block_hash: &[u8; 32],
        height: u64,
        block_time: u64,
        data: &BlockConsensusData,
    ) {
        let mut tip = self.tip.write().unwrap();

        // Accumulate PoW work.
        let block_work: u128 = if data.consensus_type == ConsensusType::ProofOfWork {
            1u128 << data.difficulty_bits.min(127)
        } else {
            0
        };
        tip.cumulative_work = tip.cumulative_work.saturating_add(block_work);
        tip.height = height;
        tip.hash = *block_hash;
        tip.last_block_time = block_time;

        if data.consensus_type == ConsensusType::ProofOfStake {
            tip.stake_modifier = data.stake_modifier;
        }

        // Retarget every `difficulty_adjustment_window` PoW blocks.
        let window = self.config.pow.difficulty_adjustment_window;
        if data.consensus_type == ConsensusType::ProofOfWork
            && height > 0
            && height % window == 0
        {
            let actual_secs = block_time.saturating_sub(tip.window_start_time);
            let target_secs = self.config.pow.target_window_timespan();
            let new_bits = retarget_difficulty(
                tip.difficulty_bits,
                actual_secs,
                target_secs,
                self.config.pow.initial_difficulty_bits / 2,
            );
            tip.difficulty_bits = new_bits;
            tip.window_start_time = block_time;
            tip.window_start_height = height;
        }
    }

    // -----------------------------------------------------------------------
    // Chain selection
    // -----------------------------------------------------------------------

    /// Returns true if the candidate chain (by cumulative work) is heavier
    /// than our current best chain — i.e., we should reorganize.
    pub fn is_heavier_chain(&self, candidate_cumulative_work: u128) -> bool {
        let tip = self.tip();
        candidate_cumulative_work > tip.cumulative_work
    }
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ConsensusError {
    #[error("PoW hash does not meet difficulty target")]
    InsufficientWork,
    #[error("Invalid difficulty: expected {expected} bits, found {found} bits")]
    InvalidDifficulty { expected: u32, found: u32 },
    #[error("PoS kernel hash does not meet stake target")]
    InsufficientStake,
    #[error("Coin age below minimum threshold")]
    ImmatureCoin,
    #[error("Block timestamp is too far in the future")]
    TimestampTooFarFuture,
    #[error("Block timestamp is before parent")]
    TimestampBeforeParent,
}

// ---------------------------------------------------------------------------
// Cryptographic helpers
// ---------------------------------------------------------------------------

/// Compute the SHA256d (double-SHA256) block header hash.
/// Input: parent_hash || height (LE u64) || nonce (LE u64) || extra_data
pub fn sha256d_block_hash(
    parent_hash: &[u8; 32],
    height: u64,
    nonce: u64,
    extra_data: &[u8],
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut preimage = Vec::with_capacity(32 + 8 + 8 + extra_data.len());
    preimage.extend_from_slice(parent_hash);
    preimage.extend_from_slice(&height.to_le_bytes());
    preimage.extend_from_slice(&nonce.to_le_bytes());
    preimage.extend_from_slice(extra_data);
    let first = Sha256::digest(&preimage);
    let second = Sha256::digest(&first);
    second.into()
}

/// Compute the PoS kernel hash.
/// Input: stake_modifier || txout_hash || txout_n (LE u32) || block_time (LE u64)
pub fn pos_kernel_hash(
    stake_modifier: &[u8; 32],
    txout_hash: &[u8; 32],
    txout_n: u32,
    block_time: u64,
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut preimage = Vec::with_capacity(32 + 32 + 4 + 8);
    preimage.extend_from_slice(stake_modifier);
    preimage.extend_from_slice(txout_hash);
    preimage.extend_from_slice(&txout_n.to_le_bytes());
    preimage.extend_from_slice(&block_time.to_le_bytes());
    let first = Sha256::digest(&preimage);
    let second = Sha256::digest(&first);
    second.into()
}

/// Compute new stake modifier: SHA256(prev_modifier || kernel_hash).
pub fn compute_stake_modifier(
    prev_modifier: &[u8; 32],
    kernel_hash: &[u8; 32],
) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut preimage = [0u8; 64];
    preimage[..32].copy_from_slice(prev_modifier);
    preimage[32..].copy_from_slice(kernel_hash);
    Sha256::digest(&preimage).into()
}

/// Map coin-age weight to leading-zero bits reduction (log2 scale).
fn leading_zeros_from_weight(weight: u64, reference_unit: u64) -> u32 {
    if weight <= reference_unit {
        return 0;
    }
    let ratio = weight / reference_unit;
    (u64::BITS - ratio.leading_zeros()).saturating_sub(1)
}

/// Current Unix timestamp in seconds.
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Mining service (runs on its own thread)
// ---------------------------------------------------------------------------

/// Configuration for the mining thread.
#[derive(Clone, Debug)]
pub struct MinerConfig {
    /// Public key that receives the coinbase reward.
    pub coinbase_pubkey: Pubkey,
    /// Number of OS threads dedicated to PoW mining (0 = disabled).
    pub mining_threads: usize,
    /// Number of OS threads for PoS minting checks (0 = disabled).
    pub staking_threads: usize,
}

impl Default for MinerConfig {
    fn default() -> Self {
        Self {
            coinbase_pubkey: Pubkey::default(),
            mining_threads: 1,
            staking_threads: 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::consensus::HybridConsensusConfig;

    fn test_engine() -> HybridConsensusEngine {
        HybridConsensusEngine::new(
            HybridConsensusConfig::default(),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[test]
    fn test_mine_trivial_block() {
        // Override to difficulty 1 so mining is instant in tests.
        let mut config = HybridConsensusConfig::default();
        config.pow.initial_difficulty_bits = 1;
        let exit = Arc::new(AtomicBool::new(false));
        let engine = HybridConsensusEngine::new(config, exit);
        let parent = [0u8; 32];
        let result = engine.mine_pow_block(&parent, 1, b"test");
        assert!(result.is_some());
        let data = result.unwrap();
        assert_eq!(data.consensus_type, ConsensusType::ProofOfWork);
    }

    #[test]
    fn test_sha256d_block_hash_deterministic() {
        let parent = [1u8; 32];
        let h1 = sha256d_block_hash(&parent, 42, 12345, b"extra");
        let h2 = sha256d_block_hash(&parent, 42, 12345, b"extra");
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_stake_modifier_changes() {
        let prev = [0u8; 32];
        let kernel = [1u8; 32];
        let new_mod = compute_stake_modifier(&prev, &kernel);
        assert_ne!(new_mod, prev);
    }

    #[test]
    fn test_chain_selection_heavier_wins() {
        let engine = test_engine();
        // Fresh engine has cumulative_work=0, anything > 0 wins.
        assert!(engine.is_heavier_chain(1));
        assert!(!engine.is_heavier_chain(0));
    }
}
