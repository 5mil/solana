//! Hybrid consensus block validation hook for the runtime.
//!
//! Called by the bank/replay stage when a new block arrives from the network
//! or is produced locally. Validates the consensus proof before executing
//! the block's transactions.

use {
    solana_sdk::{
        consensus::{
            BlockConsensusData, ConsensusType, PowConfig, PosConfig,
            hash_meets_difficulty,
        },
        genesis_config::GenesisConfig,
    },
    std::time::{SystemTime, UNIX_EPOCH},
};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum BlockValidationError {
    #[error("PoW hash does not satisfy difficulty target of {difficulty_bits} bits")]
    PowHashInvalid { difficulty_bits: u32 },

    #[error("PoW nonce produced wrong block hash")]
    PowHashMismatch,

    #[error("PoS coin age {actual} below required minimum {required}")]
    PosInsufficientCoinAge { required: u64, actual: u64 },

    #[error("Block timestamp {block_time} is before parent timestamp {parent_time}")]
    TimestampRegression { block_time: u64, parent_time: u64 },

    #[error("Block timestamp is {delta}s in the future (max allowed: {max_drift}s)")]
    TimestampTooFarFuture { delta: u64, max_drift: u64 },

    #[error("Block difficulty {found} does not match expected {expected}")]
    DifficultyMismatch { expected: u32, found: u32 },

    #[error("Coinbase reward {actual} exceeds allowed maximum {max}")]
    CoinbaseOverflow { max: u64, actual: u64 },
}

/// Maximum seconds a block timestamp may exceed local wall clock.
const MAX_FUTURE_DRIFT_SECS: u64 = 120;

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// Validate a block's consensus proof against current chain parameters.
///
/// `block_hash`   — the double-SHA256 of the block header
/// `parent_hash`  — hash of the preceding block
/// `height`       — this block's height
/// `block_time`   — Unix timestamp in the block header
/// `parent_time`  — Unix timestamp of the parent block
/// `consensus`    — the `BlockConsensusData` extracted from the block header
/// `genesis`      — chain genesis config (carries PoW/PoS parameters)
pub fn validate_block_consensus(
    block_hash: &[u8; 32],
    parent_hash: &[u8; 32],
    height: u64,
    block_time: u64,
    parent_time: u64,
    consensus: &BlockConsensusData,
    genesis: &GenesisConfig,
) -> Result<(), BlockValidationError> {
    let now = unix_now();

    // 1. Timestamp sanity checks.
    if block_time < parent_time {
        return Err(BlockValidationError::TimestampRegression {
            block_time,
            parent_time,
        });
    }
    if block_time > now + MAX_FUTURE_DRIFT_SECS {
        return Err(BlockValidationError::TimestampTooFarFuture {
            delta: block_time - now,
            max_drift: MAX_FUTURE_DRIFT_SECS,
        });
    }

    match consensus.consensus_type {
        ConsensusType::ProofOfWork => {
            validate_pow(block_hash, consensus, &genesis.consensus_config.pow)?
        }
        ConsensusType::ProofOfStake => {
            validate_pos(consensus, &genesis.consensus_config.pos)?
        }
    }

    Ok(())
}

fn validate_pow(
    block_hash: &[u8; 32],
    consensus: &BlockConsensusData,
    pow: &PowConfig,
) -> Result<(), BlockValidationError> {
    if !hash_meets_difficulty(block_hash, consensus.difficulty_bits) {
        return Err(BlockValidationError::PowHashInvalid {
            difficulty_bits: consensus.difficulty_bits,
        });
    }
    Ok(())
}

fn validate_pos(
    consensus: &BlockConsensusData,
    pos: &PosConfig,
) -> Result<(), BlockValidationError> {
    let required = pos.min_stake_lamports.saturating_mul(pos.min_coin_age_secs);
    if consensus.coin_age_consumed < required {
        return Err(BlockValidationError::PosInsufficientCoinAge {
            required,
            actual: consensus.coin_age_consumed,
        });
    }
    Ok(())
}

/// Compute the coinbase reward ceiling for a given block height.
pub fn max_coinbase_reward(height: u64, genesis: &GenesisConfig) -> u64 {
    genesis.pow_block_reward(height)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        consensus::{BlockConsensusData, ConsensusType},
        genesis_config::GenesisConfig,
    };

    fn good_pow_block() -> ([u8; 32], BlockConsensusData) {
        // Build a hash that satisfies 1-bit difficulty (MSB = 0).
        let mut hash = [0u8; 32];
        hash[0] = 0x00;
        let data = BlockConsensusData {
            consensus_type: ConsensusType::ProofOfWork,
            nonce: 0,
            difficulty_bits: 1,
            stake_modifier: [0u8; 32],
            coin_age_consumed: 0,
        };
        (hash, data)
    }

    #[test]
    fn test_valid_pow_block() {
        let mut genesis = GenesisConfig::default();
        genesis.consensus_config.pow.initial_difficulty_bits = 1;
        let (hash, mut data) = good_pow_block();
        data.difficulty_bits = 1;
        let now = unix_now();
        let result = validate_block_consensus(&hash, &[0u8;32], 1, now, now - 60, &data, &genesis);
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn test_timestamp_regression_rejected() {
        let genesis = GenesisConfig::default();
        let (hash, data) = good_pow_block();
        // block_time < parent_time
        let result = validate_block_consensus(&hash, &[0u8;32], 1, 1000, 2000, &data, &genesis);
        assert!(matches!(result, Err(BlockValidationError::TimestampRegression { .. })));
    }

    #[test]
    fn test_future_timestamp_rejected() {
        let genesis = GenesisConfig::default();
        let (hash, data) = good_pow_block();
        let far_future = unix_now() + 10_000;
        let result = validate_block_consensus(&hash, &[0u8;32], 1, far_future, far_future - 60, &data, &genesis);
        assert!(matches!(result, Err(BlockValidationError::TimestampTooFarFuture { .. })));
    }
}
