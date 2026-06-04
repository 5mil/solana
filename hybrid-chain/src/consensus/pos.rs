use crate::params::CHAIN_PARAMS;

/// Coin-age = coins * seconds_held (capped at pos_coin_age_max)
/// Returns coin-age in coin-seconds
pub fn calculate_coin_age(coins: u64, seconds_held: u64) -> u64 {
    let capped = seconds_held.min(CHAIN_PARAMS.pos_coin_age_max);
    coins.saturating_mul(capped)
}

/// PoS stake weight — used as probability basis for minting
/// Higher coin-age = higher chance to mint next PoS block
pub fn stake_weight(coins: u64, seconds_held: u64) -> u64 {
    if seconds_held < CHAIN_PARAMS.pos_coin_age_min {
        return 0; // coins haven't matured yet
    }
    if coins < CHAIN_PARAMS.min_stake {
        return 0; // below minimum stake threshold
    }
    calculate_coin_age(coins, seconds_held)
}

/// Calculate PoS block reward based on coin-age consumed
/// Reward = coins * annual_rate * (days_held / 365)
pub fn pos_reward(coins: u64, seconds_held: u64) -> u64 {
    let days_held = seconds_held.min(CHAIN_PARAMS.pos_coin_age_max) as f64 / 86400.0;
    let annual = CHAIN_PARAMS.pos_annual_rate;
    let reward = (coins as f64) * annual * (days_held / 365.0);
    reward as u64
}

/// Validate that a PoS minter has sufficient stake and coin age
pub fn validate_stake(coins: u64, seconds_held: u64) -> Result<(), &'static str> {
    if coins < CHAIN_PARAMS.min_stake {
        return Err("Insufficient stake: below minimum");
    }
    if seconds_held < CHAIN_PARAMS.pos_coin_age_min {
        return Err("Coin age too low: coins must be held at least 1 day");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coin_age_capped() {
        let max_age = CHAIN_PARAMS.pos_coin_age_max;
        let age = calculate_coin_age(1000, max_age + 1000);
        let expected = calculate_coin_age(1000, max_age);
        assert_eq!(age, expected);
    }

    #[test]
    fn test_stake_weight_immature() {
        let weight = stake_weight(10_000_0000_0000, 3600); // 1 hour held
        assert_eq!(weight, 0); // too young
    }

    #[test]
    fn test_pos_reward_calculation() {
        // 1000 coins held 365 days at 5% = 50 coins reward
        let coins = 1000_0000_0000u64; // 1000 coins in base units
        let reward = pos_reward(coins, 365 * 86400);
        let expected = (coins as f64 * 0.05) as u64;
        assert!((reward as i64 - expected as i64).abs() < 100_000);
    }
}
