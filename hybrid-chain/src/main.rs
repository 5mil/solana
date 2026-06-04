mod consensus;
mod chain;
mod wallet;
mod params;

use chain::blockchain::Blockchain;
use params::CHAIN_PARAMS;

fn main() {
    env_logger::init();
    println!("=== Hybrid SHA256d PoW/PoS Node ===");
    println!("Chain: {}", CHAIN_PARAMS.name);
    println!("Ticker: {}", CHAIN_PARAMS.ticker);
    println!("Max Supply: {} coins", CHAIN_PARAMS.max_supply);
    println!("PoW Block Reward: {} coins", CHAIN_PARAMS.pow_block_reward);
    println!("PoS Annual Rate: {}%", CHAIN_PARAMS.pos_annual_rate * 100.0);

    let mut blockchain = Blockchain::new();
    println!("\nGenesis block created: {:?}", blockchain.tip_hash());

    // Mine a PoW block
    let pow_block = blockchain.mine_pow_block("miner_address_here");
    println!("Mined PoW block #{}: {}", pow_block.header.height, hex::encode(&pow_block.hash()));

    // Stake a PoS block (requires min stake)
    let stake_result = blockchain.mint_pos_block("staker_address_here", 1000);
    match stake_result {
        Ok(pos_block) => println!("Minted PoS block #{}: {}", pos_block.header.height, hex::encode(&pos_block.hash())),
        Err(e) => println!("PoS mint failed: {}", e),
    }
}
