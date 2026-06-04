use crate::params::CHAIN_PARAMS;

/// Retarget difficulty every N blocks (like Bitcoin's 2016-block window)
/// Returns new difficulty in leading zero bits
pub fn retarget(
    current_difficulty: u32,
    actual_timespan: u64,   // seconds for last window
    target_timespan: u64,   // expected seconds for window
) -> u32 {
    // Clamp adjustment to 4x in either direction (Bitcoin-style)
    let clamped_actual = actual_timespan
        .max(target_timespan / 4)
        .min(target_timespan * 4);

    // New difficulty = current * (target / actual)
    // We work in integer approximation
    let ratio = (clamped_actual * 1000) / target_timespan;

    // ratio < 1000 means blocks came too fast → increase difficulty
    // ratio > 1000 means blocks came too slow → decrease difficulty
    if ratio < 1000 {
        // Increase by up to 1 bit
        (current_difficulty + 1).min(240)
    } else if ratio > 1000 {
        // Decrease by up to 1 bit
        current_difficulty.saturating_sub(1).max(CHAIN_PARAMS.initial_difficulty)
    } else {
        current_difficulty
    }
}

/// Calculate the target timespan for the adjustment window
pub fn pow_target_timespan() -> u64 {
    CHAIN_PARAMS.difficulty_adjustment_window * CHAIN_PARAMS.pow_target_block_time
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_difficulty_increases_when_fast() {
        let target = pow_target_timespan();
        let result = retarget(20, target / 2, target); // blocks came twice as fast
        assert!(result > 20);
    }

    #[test]
    fn test_difficulty_decreases_when_slow() {
        let target = pow_target_timespan();
        let result = retarget(20, target * 2, target); // blocks came twice as slow
        assert!(result < 20);
    }
}
