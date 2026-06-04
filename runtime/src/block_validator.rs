//! Hybrid consensus block validation hook for the runtime.
//! Called by the bank/replay stage when a new block arrives.

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

const MAX_FUTURE_DRIFT_SECS: u64 = 120;

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
    if block_time < parent_time {
        return Err(BlockValidationError::TimestampRegression { block_time, parent_time });
    }
    if block_time > now + MAX_FUTURE_DRIFT_SECS {
        return Err(BlockValidationError::TimestampTooFarFuture {
            delta: block_time - now,
            max_drift: MAX_FUTURE_DRIFT_SECS,
        });
    }
    match consensus.consensus_type {
        ConsensusType::ProofOfWork => {
            if !hash_meets_difficulty(block_hash, consensus.difficulty_bits) {
                return Err(BlockValidationError::PowHashInvalid { difficulty_bits: consensus.difficulty_bits });
            }
        }
        ConsensusType::ProofOfStake => {
            let pos = &genesis.consensus_config.pos;
            let required = pos.min_stake_lamports.saturating_mul(pos.min_coin_age_secs);
            if consensus.coin_age_consumed < required {
                return Err(BlockValidationError::PosInsufficientCoinAge {
                    required,
                    actual: consensus.coin_age_consumed,
                });
            }
        }
    }
    Ok(())
}

pub fn max_coinbase_reward(height: u64, genesis: &GenesisConfig) -> u64 {
    genesis.pow_block_reward(height)
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::{
        consensus::{BlockConsensusData, ConsensusType},
        genesis_config::GenesisConfig,
    };

    #[test]
    fn test_valid_pow_block() {
        let genesis = GenesisConfig::default();
        let mut hash = [0u8; 32]; // all zeros satisfies any difficulty
        let data = BlockConsensusData {
            consensus_type: ConsensusType::ProofOfWork,
            nonce: 0,
            difficulty_bits: 1,
            stake_modifier: [0u8;32],
            coin_age_consumed: 0,
        };
        let now = unix_now();
        assert!(validate_block_consensus(&hash, &[0u8;32], 1, now, now-60, &data, &genesis).is_ok());
    }

    #[test]
    fn test_timestamp_regression() {
        let genesis = GenesisConfig::default();
        let hash = [0u8; 32];
        let data = BlockConsensusData::default();
        let r = validate_block_consensus(&hash, &[0u8;32], 1, 1000, 2000, &data, &genesis);
        assert!(matches!(r, Err(BlockValidationError::TimestampRegression{..})));
    }
}
