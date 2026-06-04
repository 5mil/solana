/// Global chain parameters — edit these to configure your coin.
pub struct ChainParams {
    pub name: &'static str,
    pub ticker: &'static str,
    pub max_supply: u64,
    pub pow_block_reward: u64,        // in base units (satoshis)
    pub pos_annual_rate: f64,         // e.g. 0.05 = 5% per year
    pub min_stake: u64,               // minimum coins to stake
    pub pow_target_block_time: u64,   // seconds
    pub pos_target_block_time: u64,   // seconds
    pub difficulty_adjustment_window: u64, // blocks
    pub coin_maturity: u64,           // blocks before PoW reward spendable
    pub pos_coin_age_min: u64,        // minimum coin age in seconds to stake
    pub pos_coin_age_max: u64,        // cap coin age accumulation (seconds)
    pub initial_difficulty: u32,      // leading zero bits
    pub genesis_timestamp: i64,
    pub genesis_message: &'static str,
}

pub const CHAIN_PARAMS: ChainParams = ChainParams {
    name: "HybridChain",
    ticker: "HYB",
    max_supply: 21_000_000,
    pow_block_reward: 50_0000_0000,   // 50 coins in base units
    pos_annual_rate: 0.05,            // 5% annual PoS reward
    min_stake: 100_0000_0000,         // 100 coins minimum stake
    pow_target_block_time: 120,       // 2 minutes
    pos_target_block_time: 60,        // 1 minute
    difficulty_adjustment_window: 2016,
    coin_maturity: 100,
    pos_coin_age_min: 86400,          // 1 day
    pos_coin_age_max: 86400 * 90,     // 90 days max coin age
    initial_difficulty: 20,           // 20 leading zero bits
    genesis_timestamp: 1748996400,    // 2025-06-03 (project start)
    genesis_message: "HybridChain genesis - SHA256d PoW/PoS hybrid blockchain",
};
