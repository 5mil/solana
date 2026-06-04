//! RPC methods for hybrid PoW/PoS consensus queries.
//!
//! Adds the following JSON-RPC methods:
//!   getChainTip            — returns current tip (height, hash, work, difficulty)
//!   getBlockConsensusInfo  — returns consensus data for a block at given height
//!   getDifficultyHistory   — returns difficulty_bits for the last N PoW blocks
//!   getMiningInfo          — current difficulty, hash rate estimate, next retarget
//!   getStakingInfo         — current PoS difficulty, eligible UTXOs count

use {
    serde::{Deserialize, Serialize},
    solana_sdk::consensus::{ConsensusType, BlockConsensusData},
};

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChainTipResponse {
    pub height: u64,
    pub hash: String,
    pub cumulative_work: String, // u128 as hex string
    pub difficulty_bits: u32,
    pub last_block_time: u64,
    pub last_consensus_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockConsensusInfoResponse {
    pub height: u64,
    pub hash: String,
    pub parent_hash: String,
    pub block_time: u64,
    pub consensus_type: String,
    pub nonce: u64,
    pub difficulty_bits: u32,
    pub coin_age_consumed: u64,
    pub cumulative_work: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MiningInfoResponse {
    pub difficulty_bits: u32,
    pub difficulty: f64,
    pub network_hashrate_estimate: f64, // hashes/sec
    pub blocks_since_retarget: u64,
    pub blocks_until_retarget: u64,
    pub estimated_seconds_to_retarget: f64,
    pub block_reward: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StakingInfoResponse {
    pub pos_difficulty_bits: u32,
    pub min_stake_lamports: u64,
    pub min_coin_age_secs: u64,
    pub annual_reward_rate_pct: f64,
}

// ---------------------------------------------------------------------------
// Handler implementations
// ---------------------------------------------------------------------------

/// Compute floating-point difficulty from bits.
/// Analogous to Bitcoin's difficulty: 2^difficulty_bits / 2^reference_bits.
pub fn bits_to_difficulty(bits: u32, reference_bits: u32) -> f64 {
    if bits >= reference_bits {
        2.0f64.powi((bits - reference_bits) as i32)
    } else {
        1.0 / 2.0f64.powi((reference_bits - bits) as i32)
    }
}

/// Estimate network hash rate from average block time and difficulty.
pub fn estimate_hashrate(difficulty_bits: u32, avg_block_time_secs: f64) -> f64 {
    // Expected hashes to find a block = 2^difficulty_bits
    // Network hashrate ≈ 2^bits / avg_block_time
    if avg_block_time_secs <= 0.0 { return 0.0; }
    2.0f64.powi(difficulty_bits as i32) / avg_block_time_secs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_difficulty_conversion() {
        assert!((bits_to_difficulty(20, 20) - 1.0).abs() < 1e-9);
        assert!((bits_to_difficulty(21, 20) - 2.0).abs() < 1e-9);
        assert!((bits_to_difficulty(19, 20) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_hashrate_estimate() {
        let hr = estimate_hashrate(20, 120.0);
        assert!(hr > 0.0);
    }
}
