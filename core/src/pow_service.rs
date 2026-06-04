//! PoW Mining Service — spawns N threads that race to find a SHA256d nonce.
//! When a solution is found it is sent on a channel to the replay/TPU stage.

use {
    crate::consensus::{
        HybridConsensusEngine, MinerConfig, sha256d_block_hash, unix_now,
    },
    solana_sdk::{
        consensus::{BlockConsensusData, ConsensusType},
        pubkey::Pubkey,
    },
    std::{
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            mpsc::{self, Receiver, SyncSender},
            Arc,
        },
        thread::{self, JoinHandle},
    },
};

#[derive(Debug)]
pub struct SolvedPowBlock {
    pub height: u64,
    pub parent_hash: [u8; 32],
    pub block_time: u64,
    pub miner_pubkey: Pubkey,
    pub block_reward: u64,
    pub consensus_data: BlockConsensusData,
}

pub struct PowMiningService {
    threads: Vec<JoinHandle<()>>,
}

impl PowMiningService {
    pub fn new(
        engine: Arc<HybridConsensusEngine>,
        config: MinerConfig,
        exit: Arc<AtomicBool>,
    ) -> (Self, Receiver<SolvedPowBlock>) {
        let (sender, receiver) = mpsc::sync_channel::<SolvedPowBlock>(8);
        let nonce_stride = config.mining_threads as u64;
        let threads: Vec<JoinHandle<()>> = (0..config.mining_threads)
            .map(|thread_id| {
                let engine = Arc::clone(&engine);
                let sender = sender.clone();
                let exit = Arc::clone(&exit);
                let coinbase = config.coinbase_pubkey;
                let start_nonce = thread_id as u64;
                thread::Builder::new()
                    .name(format!("pow-miner-{thread_id}"))
                    .spawn(move || {
                        Self::mining_loop(engine, exit, sender, coinbase, start_nonce, nonce_stride)
                    })
                    .expect("failed to spawn mining thread")
            })
            .collect();
        (Self { threads }, receiver)
    }

    fn mining_loop(
        engine: Arc<HybridConsensusEngine>,
        exit: Arc<AtomicBool>,
        sender: SyncSender<SolvedPowBlock>,
        coinbase: Pubkey,
        start_nonce: u64,
        stride: u64,
    ) {
        let mut nonce = start_nonce;
        loop {
            if exit.load(Ordering::Relaxed) { return; }
            let tip = engine.tip();
            let height = tip.height + 1;
            let reward = engine.config_ref().pow.block_reward_at_height(height);
            let hash = sha256d_block_hash(&tip.hash, height, nonce, b"");
            use solana_sdk::consensus::hash_meets_difficulty;
            if hash_meets_difficulty(&hash, tip.difficulty_bits) {
                let data = BlockConsensusData {
                    consensus_type: ConsensusType::ProofOfWork,
                    nonce,
                    difficulty_bits: tip.difficulty_bits,
                    stake_modifier: [0u8; 32],
                    coin_age_consumed: 0,
                };
                let _ = sender.try_send(SolvedPowBlock {
                    height,
                    parent_hash: tip.hash,
                    block_time: unix_now(),
                    miner_pubkey: coinbase,
                    block_reward: reward,
                    consensus_data: data,
                });
            }
            nonce = nonce.wrapping_add(stride);
            if nonce % 100_000 == 0 { thread::yield_now(); }
        }
    }

    pub fn join(self) {
        for t in self.threads { let _ = t.join(); }
    }
}
