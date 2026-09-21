mod consensus;
mod chain;
mod wallet;
mod params;

use chain::blockchain::Blockchain;
use chain::pool::DefaultPool;
use params::CHAIN_PARAMS;

fn main() {
    env_logger::init();
    println!("=== Hybrid SHA256d PoW/PoS Node ===");
    println!("Chain: {}", CHAIN_PARAMS.name);
    println!("Ticker: {}", CHAIN_PARAMS.ticker);
    println!("Max Supply: {} coins", CHAIN_PARAMS.max_supply);
    println!("PoW Block Reward: {} coins", CHAIN_PARAMS.pow_block_reward);
    println!("PoS Annual Rate: {}%", CHAIN_PARAMS.pos_annual_rate * 100.0);
    println!("Default pool: {}", DefaultPool::name());

    let mut blockchain = Blockchain::new();
    println!("\nGenesis block created: {}", hex::encode(blockchain.tip_hash()));

    let pool = DefaultPool::new();

    let pow_block = blockchain.mine_pow_block("miner_address_here");
    println!(
        "Mined PoW block #{}: {}",
        pow_block.header.height,
        hex::encode(pow_block.hash())
    );

    match pool.submit_block("miner_address_here", &pow_block) {
        Ok(share) => println!(
            "Accepted by {}: height={} nonce={} hash={}",
            DefaultPool::name(),
            share.height,
            share.nonce,
            hex::encode(share.hash)
        ),
        Err(e) => println!("Pool rejected share: {:?}", e),
    }

    let stake_result = blockchain.mint_pos_block("staker_address_here", CHAIN_PARAMS.min_stake);
    match stake_result {
        Ok(pos_block) => println!(
            "Minted PoS block #{}: {}",
            pos_block.header.height,
            hex::encode(pos_block.hash())
        ),
        Err(e) => println!("PoS mint failed: {}", e),
    }

    println!(
        "Pool stats: accepted={} rejected={}",
        pool.accepted_count(),
        pool.rejected_count()
    );
}
