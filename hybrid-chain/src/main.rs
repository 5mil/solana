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

fn default_data_path() -> PathBuf { PathBuf::from("data/chain.bin") }

fn print_banner() {
    println!("=== Orthal ===");
    println!("Chain: {}", CHAIN_PARAMS.name);
    println!("Ticker: {}  notes: compact actions, issued tickers on the same bundle", CHAIN_PARAMS.ticker);
}

fn mine_and_persist(path: &Path) {
    print_banner();
    let mut blockchain = Blockchain::new();
    let pool = DefaultPool::new();
    let seed = "miner-wallet-seed";
    let pow_block = blockchain.mine_pow_block(seed);
    let sealed = SealedPayout::from_wallet_seed(seed.as_bytes(), pow_block.header.height);
    println!("PoW #{} {}", pow_block.header.height, hex::encode(pow_block.hash()));
    println!("dest pk {}", hex::encode(sealed.dest.bytes));
    match pool.submit_block(seed, &pow_block) {
        Ok(share) => println!("pool accepted height={}", share.height),
        Err(e) => { println!("pool rejected: {:?}", e); process::exit(1); }
    }
    blockchain.save_to_path(path).unwrap_or_else(|e| { eprintln!("{:?}", e); process::exit(1); });
}

fn replay(path: &Path) {
    print_banner();
    let chain = Blockchain::load_from_path(path).unwrap_or_else(|e| { eprintln!("{:?}", e); process::exit(1); });
    chain.revalidate().unwrap_or_else(|e| { eprintln!("{:?}", e); process::exit(1); });
    println!("revalidated {} blocks", chain.height());
}

fn main() {
    env_logger::init();
    let args: Vec<String> = env::args().collect();
    let replay_mode = args.iter().any(|a| a == "--replay");
    let data_path = args.windows(2).find(|w| w[0] == "--data").map(|w| PathBuf::from(&w[1])).unwrap_or_else(default_data_path);
    if replay_mode { replay(&data_path); } else { mine_and_persist(&data_path); }
}
