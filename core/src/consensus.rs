//! Hybrid SHA256d PoW / Proof-of-Stake consensus engine for core.

use {
    solana_sdk::{
        consensus::{
            BlockConsensusData, ConsensusType, HybridConsensusConfig,
            hash_meets_difficulty, retarget_difficulty,
        },
        pubkey::Pubkey,
    },
    std::{
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            Arc, RwLock,
        },
        time::{SystemTime, UNIX_EPOCH},
    },
};

#[derive(Clone, Debug)]
pub struct ChainTip {
    pub height: u64,
    pub hash: [u8; 32],
    pub cumulative_work: u128,
    pub difficulty_bits: u32,
    pub last_block_time: u64,
    pub stake_modifier: [u8; 32],
    pub window_start_time: u64,
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

pub struct HybridConsensusEngine {
    config: HybridConsensusConfig,
    tip: RwLock<ChainTip>,
    nonce_counter: AtomicU64,
    exit: Arc<AtomicBool>,
}

impl HybridConsensusEngine {
    pub fn new(config: HybridConsensusConfig, exit: Arc<AtomicBool>) -> Self {
        let tip = ChainTip { difficulty_bits: config.pow.initial_difficulty_bits, ..ChainTip::default() };
        Self { config, tip: RwLock::new(tip), nonce_counter: AtomicU64::new(0), exit }
    }

    pub fn tip(&self) -> ChainTip { self.tip.read().unwrap().clone() }

    /// Expose config for pos_service reward calculations.
    pub fn config_ref(&self) -> &HybridConsensusConfig { &self.config }

    pub fn mine_pow_block(&self, parent_hash: &[u8; 32], height: u64, extra_data: &[u8]) -> Option<BlockConsensusData> {
        let tip = self.tip();
        let difficulty_bits = tip.difficulty_bits;
        let start_nonce = self.nonce_counter.fetch_add(1_000_000, Ordering::Relaxed);
        let mut nonce = start_nonce;
        loop {
            if self.exit.load(Ordering::Relaxed) { return None; }
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
            if nonce % 100_000 == 0 { std::thread::yield_now(); }
        }
    }

    pub fn try_pos_mint(&self, lamports: u64, held_secs: u64, coin_age_consumed: u64, kernel_hash: &[u8; 32]) -> Option<BlockConsensusData> {
        let cfg = &self.config.pos;
        let tip = self.tip();
        let weight = cfg.stake_weight(lamports, held_secs);
        if weight == 0 { return None; }
        let reference_unit: u64 = 1_000_000_000 * 86_400;
        let pos_difficulty = if weight >= reference_unit {
            tip.difficulty_bits.saturating_sub(leading_zeros_from_weight(weight, reference_unit))
        } else { tip.difficulty_bits }.max(8);
        if !hash_meets_difficulty(kernel_hash, pos_difficulty) { return None; }
        let new_stake_modifier = compute_stake_modifier(&tip.stake_modifier, kernel_hash);
        Some(BlockConsensusData {
            consensus_type: ConsensusType::ProofOfStake,
            nonce: 0,
            difficulty_bits: tip.difficulty_bits,
            stake_modifier: new_stake_modifier,
            coin_age_consumed,
        })
    }

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
            ConsensusType::ProofOfWork => self.validate_pow_block(parent_hash, height, consensus_data, extra_data)?,
            ConsensusType::ProofOfStake => {
                let tip = self.tip();
                if consensus_data.difficulty_bits != tip.difficulty_bits {
                    return Err(ConsensusError::InvalidDifficulty { expected: tip.difficulty_bits, found: consensus_data.difficulty_bits });
                }
            }
        }
        self.update_tip(block_hash, height, block_time, consensus_data);
        Ok(())
    }

    fn validate_pow_block(&self, parent_hash: &[u8; 32], height: u64, data: &BlockConsensusData, extra_data: &[u8]) -> Result<(), ConsensusError> {
        let tip = self.tip();
        if data.difficulty_bits != tip.difficulty_bits {
            return Err(ConsensusError::InvalidDifficulty { expected: tip.difficulty_bits, found: data.difficulty_bits });
        }
        let hash = sha256d_block_hash(parent_hash, height, data.nonce, extra_data);
        if !hash_meets_difficulty(&hash, data.difficulty_bits) { return Err(ConsensusError::InsufficientWork); }
        Ok(())
    }

    fn update_tip(&self, block_hash: &[u8; 32], height: u64, block_time: u64, data: &BlockConsensusData) {
        let mut tip = self.tip.write().unwrap();
        let block_work: u128 = if data.consensus_type == ConsensusType::ProofOfWork { 1u128 << data.difficulty_bits.min(127) } else { 0 };
        tip.cumulative_work = tip.cumulative_work.saturating_add(block_work);
        tip.height = height;
        tip.hash = *block_hash;
        tip.last_block_time = block_time;
        if data.consensus_type == ConsensusType::ProofOfStake { tip.stake_modifier = data.stake_modifier; }
        let window = self.config.pow.difficulty_adjustment_window;
        if data.consensus_type == ConsensusType::ProofOfWork && height > 0 && height % window == 0 {
            let actual_secs = block_time.saturating_sub(tip.window_start_time);
            let target_secs = self.config.pow.target_window_timespan();
            let new_bits = retarget_difficulty(tip.difficulty_bits, actual_secs, target_secs, self.config.pow.initial_difficulty_bits / 2);
            tip.difficulty_bits = new_bits;
            tip.window_start_time = block_time;
            tip.window_start_height = height;
        }
    }

    pub fn is_heavier_chain(&self, candidate_cumulative_work: u128) -> bool {
        candidate_cumulative_work > self.tip().cumulative_work
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConsensusError {
    #[error("PoW hash does not meet difficulty target")]
    InsufficientWork,
    #[error("Invalid difficulty: expected {expected} bits, found {found} bits")]
    InvalidDifficulty { expected: u32, found: u32 },
    #[error("PoS kernel hash does not meet stake target")]
    InsufficientStake,
}

pub fn sha256d_block_hash(parent_hash: &[u8; 32], height: u64, nonce: u64, extra_data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut preimage = Vec::with_capacity(32 + 8 + 8 + extra_data.len());
    preimage.extend_from_slice(parent_hash);
    preimage.extend_from_slice(&height.to_le_bytes());
    preimage.extend_from_slice(&nonce.to_le_bytes());
    preimage.extend_from_slice(extra_data);
    let first = Sha256::digest(&preimage);
    Sha256::digest(&first).into()
}

pub fn pos_kernel_hash(stake_modifier: &[u8; 32], txout_hash: &[u8; 32], txout_n: u32, block_time: u64) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut preimage = Vec::with_capacity(32 + 32 + 4 + 8);
    preimage.extend_from_slice(stake_modifier);
    preimage.extend_from_slice(txout_hash);
    preimage.extend_from_slice(&txout_n.to_le_bytes());
    preimage.extend_from_slice(&block_time.to_le_bytes());
    let first = Sha256::digest(&preimage);
    Sha256::digest(&first).into()
}

pub fn compute_stake_modifier(prev: &[u8; 32], kernel: &[u8; 32]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut preimage = [0u8; 64];
    preimage[..32].copy_from_slice(prev);
    preimage[32..].copy_from_slice(kernel);
    Sha256::digest(&preimage).into()
}

fn leading_zeros_from_weight(weight: u64, reference_unit: u64) -> u32 {
    if weight <= reference_unit { return 0; }
    let ratio = weight / reference_unit;
    (u64::BITS - ratio.leading_zeros()).saturating_sub(1)
}

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[derive(Clone, Debug)]
pub struct MinerConfig {
    pub coinbase_pubkey: Pubkey,
    pub mining_threads: usize,
    pub staking_threads: usize,
}

impl Default for MinerConfig {
    fn default() -> Self {
        Self { coinbase_pubkey: Pubkey::default(), mining_threads: 1, staking_threads: 1 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::consensus::HybridConsensusConfig;
    use std::sync::atomic::AtomicBool;

    fn test_engine() -> HybridConsensusEngine {
        HybridConsensusEngine::new(HybridConsensusConfig::default(), Arc::new(AtomicBool::new(false)))
    }

    #[test]
    fn test_chain_selection() {
        let engine = test_engine();
        assert!(engine.is_heavier_chain(1));
        assert!(!engine.is_heavier_chain(0));
    }

    #[test]
    fn test_sha256d_deterministic() {
        let h1 = sha256d_block_hash(&[1u8;32], 42, 999, b"data");
        let h2 = sha256d_block_hash(&[1u8;32], 42, 999, b"data");
        assert_eq!(h1, h2);
    }
}
