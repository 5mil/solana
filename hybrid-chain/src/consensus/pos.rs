use crate::params::CHAIN_PARAMS;

/// Coin-age = coins * seconds_held (capped at pos_coin_age_max)
pub fn calculate_coin_age(coins: u64, seconds_held: u64) -> u64 {
    let capped = seconds_held.min(CHAIN_PARAMS.pos_coin_age_max);
    coins.saturating_mul(capped)
}

pub fn stake_weight(coins: u64, seconds_held: u64) -> u64 {
    if seconds_held < CHAIN_PARAMS.pos_coin_age_min {
        return 0;
    }
    if coins < CHAIN_PARAMS.min_stake {
        return 0;
    }
    calculate_coin_age(coins, seconds_held)
}

pub fn pos_reward(coins: u64, seconds_held: u64) -> u64 {
    let days_held = seconds_held.min(CHAIN_PARAMS.pos_coin_age_max) as f64 / 86400.0;
    let annual = CHAIN_PARAMS.pos_annual_rate;
    let reward = (coins as f64) * annual * (days_held / 365.0);
    reward as u64
}

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
        let weight = stake_weight(10_000_0000_0000, 3600);
        assert_eq!(weight, 0);
    }

    #[test]
    fn test_pos_reward_calculation() {
        let coins = 1000_0000_0000u64;
        let held = CHAIN_PARAMS.pos_coin_age_max;
        let reward = pos_reward(coins, held);
        let expected = (coins as f64 * CHAIN_PARAMS.pos_annual_rate * (held as f64 / 86400.0 / 365.0)) as u64;
        assert!((reward as i64 - expected as i64).abs() < 100_000, "reward={reward} expected={expected}");
    }
}
