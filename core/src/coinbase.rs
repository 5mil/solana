//! Coinbase transaction builder for hybrid PoW/PoS blocks.
//!
//! Every PoW block includes a coinbase transaction that credits the
//! miner's pubkey with the block reward (halving schedule).
//! Every PoS block includes a staking-reward transaction that credits
//! the staker with interest proportional to their coin age.

use solana_sdk::{
    consensus::ConsensusType,
    pubkey::Pubkey,
    system_instruction,
    transaction::Transaction,
};

/// Initial block reward in lamports (50 coins, matching Bitcoin genesis).
pub const INITIAL_BLOCK_REWARD_LAMPORTS: u64 = 50_000_000_000;
/// Halve the reward every 210,000 PoW blocks.
pub const HALVING_INTERVAL: u64 = 210_000;
/// Annual PoS interest rate in basis points (100 = 1%).
pub const POS_ANNUAL_RATE_BPS: u64 = 500; // 5% APY

/// Compute the coinbase reward for a PoW block at the given height.
pub fn pow_block_reward(height: u64) -> u64 {
    let halvings = height / HALVING_INTERVAL;
    if halvings >= 64 {
        return 0; // reward fully depleted
    }
    INITIAL_BLOCK_REWARD_LAMPORTS >> halvings
}

/// Compute the PoS staking reward for coin age consumed.
///
/// `lamports`   — stake size
/// `held_secs`  — seconds the coin was held
pub fn pos_staking_reward(lamports: u64, held_secs: u64) -> u64 {
    // reward = lamports * rate * held_secs / (365.25 * 86400)
    // Using integer arithmetic with basis points to avoid floats.
    let year_secs: u64 = 31_557_600; // 365.25 * 86400
    lamports
        .saturating_mul(POS_ANNUAL_RATE_BPS)
        .saturating_mul(held_secs)
        / year_secs
        / 10_000
}

/// Build an unsigned coinbase `Transaction` for a PoW block.
/// The system program transfers the reward from the fee-pool to the miner.
pub fn build_pow_coinbase(miner: &Pubkey, height: u64) -> Option<Transaction> {
    let reward = pow_block_reward(height);
    if reward == 0 {
        return None;
    }
    // Coinbase is a special transfer from the null account (fee pool).
    // In practice the runtime will credit the miner via bank.reward_miners().
    // This returns the human-readable intent for ledger recording.
    Some(Transaction::new_unsigned(
        solana_sdk::message::Message::new(
            &[system_instruction::transfer(
                &Pubkey::default(), // fee pool / treasury
                miner,
                reward,
            )],
            Some(miner),
        ),
    ))
}

/// Build an unsigned staking-reward `Transaction` for a PoS block.
pub fn build_pos_reward(staker: &Pubkey, lamports: u64, held_secs: u64) -> Option<Transaction> {
    let reward = pos_staking_reward(lamports, held_secs);
    if reward == 0 {
        return None;
    }
    Some(Transaction::new_unsigned(
        solana_sdk::message::Message::new(
            &[system_instruction::transfer(
                &Pubkey::default(),
                staker,
                reward,
            )],
            Some(staker),
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_halving_schedule() {
        assert_eq!(pow_block_reward(0), INITIAL_BLOCK_REWARD_LAMPORTS);
        assert_eq!(pow_block_reward(HALVING_INTERVAL), INITIAL_BLOCK_REWARD_LAMPORTS / 2);
        assert_eq!(pow_block_reward(HALVING_INTERVAL * 2), INITIAL_BLOCK_REWARD_LAMPORTS / 4);
        assert_eq!(pow_block_reward(HALVING_INTERVAL * 64), 0);
    }

    #[test]
    fn test_pos_reward_proportional() {
        let one_year = 31_557_600u64;
        let lamports = 1_000_000_000u64; // 1 SOL
        let reward = pos_staking_reward(lamports, one_year);
        // 5% of 1 SOL = 50_000_000 lamports (within integer rounding)
        assert!(reward >= 49_000_000 && reward <= 51_000_000,
            "reward={reward}");
    }

    #[test]
    fn test_zero_reward_after_64_halvings() {
        assert_eq!(pow_block_reward(u64::MAX), 0);
    }
}
