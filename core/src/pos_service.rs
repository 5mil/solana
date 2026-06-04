//! PoS minting service — checks UTXO eligibility and mints PoS blocks.

use {
    crate::consensus::{
        HybridConsensusEngine, pos_kernel_hash, unix_now,
    },
    solana_sdk::{
        consensus::{BlockConsensusData, ConsensusType},
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

#[derive(Clone, Debug)]
pub struct StakeUtxo {
    pub owner: Pubkey,
    pub lamports: u64,
    pub confirmation_time: u64,
    pub txout_hash: [u8; 32],
    pub txout_n: u32,
}

impl StakeUtxo {
    pub fn held_secs(&self, now: u64) -> u64 {
        now.saturating_sub(self.confirmation_time)
    }
}

#[derive(Debug)]
pub struct MintedPosBlock {
    pub height: u64,
    pub parent_hash: [u8; 32],
    pub block_time: u64,
    pub staker_pubkey: Pubkey,
    pub staking_reward: u64,
    pub coin_age_consumed: u64,
    pub consensus_data: BlockConsensusData,
}

pub struct PosMintingService {
    thread: JoinHandle<()>,
}

impl PosMintingService {
    pub fn new(
        engine: Arc<HybridConsensusEngine>,
        exit: Arc<AtomicBool>,
        utxo_source: Arc<dyn Fn() -> Vec<StakeUtxo> + Send + Sync + 'static>,
    ) -> (Self, Receiver<MintedPosBlock>) {
        let (sender, receiver) = mpsc::sync_channel::<MintedPosBlock>(8);
        let thread = thread::Builder::new()
            .name("pos-minter".to_string())
            .spawn(move || Self::minting_loop(engine, exit, sender, utxo_source))
            .expect("failed to spawn PoS minting thread");
        (Self { thread }, receiver)
    }

    fn minting_loop(
        engine: Arc<HybridConsensusEngine>,
        exit: Arc<AtomicBool>,
        sender: SyncSender<MintedPosBlock>,
        utxo_source: Arc<dyn Fn() -> Vec<StakeUtxo> + Send + Sync + 'static>,
    ) {
        while !exit.load(Ordering::Relaxed) {
            let tip = engine.tip();
            let now = unix_now();
            for utxo in utxo_source().iter() {
                if exit.load(Ordering::Relaxed) { return; }
                let held_secs = utxo.held_secs(now);
                let coin_age = engine.config_ref().pos.coin_age(utxo.lamports, held_secs);
                let kernel = pos_kernel_hash(&tip.stake_modifier, &utxo.txout_hash, utxo.txout_n, now);
                if let Some(data) = engine.try_pos_mint(utxo.lamports, held_secs, coin_age, &kernel) {
                    let reward = engine.config_ref().pos.staking_reward(utxo.lamports, held_secs);
                    let _ = sender.try_send(MintedPosBlock {
                        height: tip.height + 1,
                        parent_hash: tip.hash,
                        block_time: now,
                        staker_pubkey: utxo.owner,
                        staking_reward: reward,
                        coin_age_consumed: coin_age,
                        consensus_data: data,
                    });
                    break;
                }
            }
            thread::sleep(Duration::from_millis(500));
        }
    }

    pub fn join(self) {
        let _ = self.thread.join();
    }
}
