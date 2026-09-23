mod consensus;
mod chain;
mod wallet;
mod params;
mod notes;

use chain::blockchain::Blockchain;
use chain::pool::DefaultPool;
use notes::payout::SealedPayout;
use params::CHAIN_PARAMS;
use std::env;
use std::path::{Path, PathBuf};
use std::process;

fn default_data_path() -> PathBuf {
    PathBuf::from("data/chain.bin")
}

fn print_banner() {
    println!("=== Hybrid SHA256d PoW/PoS Node ===");
    println!("Chain: {}", CHAIN_PARAMS.name);
    println!("Ticker: {}", CHAIN_PARAMS.ticker);
    println!("Max Supply: {} coins", CHAIN_PARAMS.max_supply);
    println!("Default pool: {}", DefaultPool::name());
    println!("Notes: nullifier spends + living set + range + binding");
}

fn mine_and_persist(path: &Path) {
    print_banner();
    let mut blockchain = Blockchain::new();
    println!(
        "\nGenesis block created: {}",
        hex::encode(blockchain.tip_hash())
    );
    println!(
        "Genesis live root: {}",
        hex::encode(blockchain.launch.commitment())
    );

    let pool = DefaultPool::new();
    let miner = "miner_address_here";
    let pow_block = blockchain.mine_pow_block(miner);
    let sealed = SealedPayout::from_ticket(miner.as_bytes(), pow_block.header.height);
    println!(
        "Mined PoW block #{}: {}",
        pow_block.header.height,
        hex::encode(pow_block.hash())
    );
    println!(
        "Sealed payout dest: {}",
        hex::encode(sealed.dest)
    );
    println!(
        "notes_root={} tags_root={}",
        hex::encode(pow_block.header.notes_root),
        hex::encode(pow_block.header.tags_root)
    );

    match pool.submit_block(miner, &pow_block) {
        Ok(share) => println!(
            "Accepted by {}: height={} nonce={} hash={}",
            DefaultPool::name(),
            share.height,
            share.nonce,
            hex::encode(share.hash)
        ),
        Err(e) => {
            println!("Pool rejected share: {:?}", e);
            process::exit(1);
        }
    }

    blockchain.save_to_path(path).unwrap_or_else(|e| {
        eprintln!("failed to persist chain: {:?}", e);
        process::exit(1);
    });
    println!("Persisted chain to {}", path.display());
}

fn replay(path: &Path) {
    print_banner();
    println!("\nReplaying chain from {}", path.display());
    let chain = Blockchain::load_from_path(path).unwrap_or_else(|e| {
        eprintln!("failed to load chain: {:?}", e);
        process::exit(1);
    });
    chain.revalidate().unwrap_or_else(|e| {
        eprintln!("revalidation failed: {:?}", e);
        process::exit(1);
    });
    println!(
        "Revalidated {} blocks. Tip: {}",
        chain.height(),
        hex::encode(chain.tip_hash())
    );
    if chain.height() < 2 {
        eprintln!("expected at least genesis + one PoW block");
        process::exit(1);
    }
    let pow = &chain.blocks[1];
    println!(
        "Stored PoW block #{} still valid: {}",
        pow.header.height,
        hex::encode(pow.hash())
    );
}

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    let replay_mode = args.iter().any(|a| a == "--replay");
    let data_path = args
        .windows(2)
        .find(|w| w[0] == "--data")
        .map(|w| PathBuf::from(&w[1]))
        .unwrap_or_else(default_data_path);

    if replay_mode {
        replay(&data_path);
    } else {
        mine_and_persist(&data_path);
    }
}
