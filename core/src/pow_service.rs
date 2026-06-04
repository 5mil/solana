//! PoW mining service — runs the SHA256d mining loop on dedicated threads.
//! Replaces the PoH service as the block-production driver for PoW blocks.

use {
    crate::consensus::{
        HybridConsensusEngine, MinerConfig, sha256d_block_hash, unix_now,
    },
    solana_sdk::{
        consensus::{BlockConsensusData, ConsensusType, hash_meets_difficulty},
        pubkey::Pubkey,
    },
    std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver, SyncSender},
            Arc,
        },
        thread::{self, JoinHandle},
        time::Duration,
    },
};

// ---------------------------------------------------------------------------
// Messages produced by the mining service
// ---------------------------------------------------------------------------

/// A solved PoW block header ready for the banking/replay stage to finalize.
#[derive(Debug)]
pub struct SolvedPowBlock {
    pub height: u64,
    pub parent_hash: [u8; 32],
    pub nonce: u64,
    pub difficulty_bits: u32,
    pub block_time: u64,
    pub coinbase_pubkey: Pubkey,
    pub consensus_data: BlockConsensusData,
}

// ---------------------------------------------------------------------------
// Mining service
// ---------------------------------------------------------------------------

pub struct PowMiningService {
    threads: Vec<JoinHandle<()>>,
}

impl PowMiningService {
    /// Spawn `config.mining_threads` mining threads.
    /// Each thread races to find a nonce; the winner sends on `solved_sender`.
    pub fn new(
        engine: Arc<HybridConsensusEngine>,
        config: MinerConfig,
        exit: Arc<AtomicBool>,
    ) -> (Self, Receiver<SolvedPowBlock>) {
        let (sender, receiver) = mpsc::sync_channel::<SolvedPowBlock>(8);
        let mut threads = Vec::with_capacity(config.mining_threads);

        for thread_id in 0..config.mining_threads {
            let engine = Arc::clone(&engine);
            let exit = Arc::clone(&exit);
            let sender = sender.clone();
            let coinbase_pubkey = config.coinbase_pubkey;

            let handle = thread::Builder::new()
                .name(format!("pow-miner-{thread_id}"))
                .spawn(move || {
                    Self::mining_loop(engine, exit, sender, coinbase_pubkey, thread_id as u64);
                })
                .expect("failed to spawn mining thread");

            threads.push(handle);
        }

        (Self { threads }, receiver)
    }

    fn mining_loop(
        engine: Arc<HybridConsensusEngine>,
        exit: Arc<AtomicBool>,
        sender: SyncSender<SolvedPowBlock>,
        coinbase_pubkey: Pubkey,
        thread_offset: u64,
    ) {
        while !exit.load(Ordering::Relaxed) {
            let tip = engine.tip();
            let height = tip.height + 1;
            let parent_hash = tip.hash;
            let difficulty_bits = tip.difficulty_bits;
            let block_time = unix_now();

            // Each thread starts from a different nonce range to avoid collisions.
            let nonce_start = thread_offset.wrapping_mul(u64::MAX / 8);
            let mut nonce = nonce_start;

            loop {
                if exit.load(Ordering::Relaxed) {
                    return;
                }

                // Re-check tip: if another thread found a block, restart on new tip.
                let current_tip = engine.tip();
                if current_tip.height != tip.height {
                    break; // tip advanced, restart
                }

                let hash = sha256d_block_hash(&parent_hash, height, nonce, &[]);

                if hash_meets_difficulty(&hash, difficulty_bits) {
                    let consensus_data = BlockConsensusData {
                        consensus_type: ConsensusType::ProofOfWork,
                        nonce,
                        difficulty_bits,
                        stake_modifier: [0u8; 32],
                        coin_age_consumed: 0,
                    };

                    let solved = SolvedPowBlock {
                        height,
                        parent_hash,
                        nonce,
                        difficulty_bits,
                        block_time,
                        coinbase_pubkey,
                        consensus_data,
                    };

                    // Best-effort send; if the channel is full, we discard
                    // (another thread likely found a solution first).
                    let _ = sender.try_send(solved);
                    break; // restart on the new tip
                }

                nonce = nonce.wrapping_add(1);

                if nonce % 100_000 == 0 {
                    std::thread::yield_now();
                }
            }
        }
    }

    pub fn join(self) {
        for t in self.threads {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::HybridConsensusEngine;
    use solana_sdk::consensus::HybridConsensusConfig;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn test_mining_service_produces_block() {
        let mut config = HybridConsensusConfig::default();
        config.pow.initial_difficulty_bits = 1; // trivial difficulty for test
        let exit = Arc::new(AtomicBool::new(false));
        let engine = Arc::new(HybridConsensusEngine::new(config, Arc::clone(&exit)));
        let miner_cfg = MinerConfig {
            coinbase_pubkey: Pubkey::default(),
            mining_threads: 1,
            staking_threads: 0,
        };
        let (service, rx) = PowMiningService::new(Arc::clone(&engine), miner_cfg, Arc::clone(&exit));
        let solved = rx.recv_timeout(Duration::from_secs(10));
        exit.store(true, Ordering::Relaxed);
        service.join();
        assert!(solved.is_ok());
    }
}
