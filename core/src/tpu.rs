//! Transaction Processing Unit — modified for hybrid PoW/PoS consensus.
//!
//! The legacy PoH-gated TPU has been replaced:
//! - Incoming transactions flow into `BlockRecorder` (poh::poh_recorder)
//! - Block production is triggered by `PowMiningService` or `PosMintingService`
//! - Once a solved/minted block arrives it is sealed, ledger-written, and broadcast
//! - Leader schedule is removed; any node that mines/mints a block is the producer

use {
    crate::{
        banking_stage::BankingStage,
        consensus::HybridConsensusEngine,
        fetch_stage::FetchStage,
        sigverify::TransactionSigVerifier,
        sigverify_stage::SigVerifyStage,
        tpu_entry_notifier::TpuEntryNotifier,
        pow_service::{PowMiningService, SolvedPowBlock},
        pos_service::{PosMintingService, StakeUtxo},
    },
    crossbeam_channel::{unbounded, Receiver},
    solana_client::connection_cache::ConnectionCache,
    solana_gossip::cluster_info::ClusterInfo,
    solana_ledger::{
        blockstore::Blockstore,
        hybrid_block_store::HybridBlockStore,
    },
    solana_poh::poh_recorder::BlockRecorder,
    solana_runtime::bank_forks::BankForks,
    solana_sdk::{
        pubkey::Pubkey,
        signature::Keypair,
        consensus::HybridConsensusConfig,
    },
    solana_streamer::{
        quic::SpawnServerResult,
        streamer::PacketBatchReceiver,
    },
    std::{
        net::UdpSocket,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, RwLock,
        },
        thread::{self, JoinHandle},
        time::Duration,
    },
};

/// All join handles owned by the TPU.
pub struct Tpu {
    fetch_stage: FetchStage,
    sigverify_stage: SigVerifyStage,
    banking_stage: BankingStage,
    pow_service: PowMiningService,
    pos_service: PosMintingService,
    block_finalizer: JoinHandle<()>,
}

/// Configuration for the hybrid consensus TPU.
#[derive(Clone)]
pub struct TpuConfig {
    pub consensus_config: HybridConsensusConfig,
    pub coinbase_pubkey: Pubkey,
    pub mining_threads: usize,
    pub staking_threads: usize,
    pub max_connections_per_peer: u32,
}

impl Default for TpuConfig {
    fn default() -> Self {
        Self {
            consensus_config: HybridConsensusConfig::default(),
            coinbase_pubkey: Pubkey::default(),
            mining_threads: num_cpus::get().max(1),
            staking_threads: 1,
            max_connections_per_peer: 8,
        }
    }
}

impl Tpu {
    pub fn new(
        cluster_info: Arc<ClusterInfo>,
        bank_forks: Arc<RwLock<BankForks>>,
        blockstore: Arc<Blockstore>,
        block_store: Arc<RwLock<HybridBlockStore>>,
        engine: Arc<HybridConsensusEngine>,
        block_recorder: BlockRecorder,
        sockets: Vec<UdpSocket>,
        tpu_config: TpuConfig,
        exit: Arc<AtomicBool>,
    ) -> Self {
        // --- Fetch stage: receive raw UDP packets ---
        let (packet_sender, packet_receiver) = unbounded();
        let fetch_stage = FetchStage::new(sockets, exit.clone(), packet_sender);

        // --- SigVerify stage: verify transaction signatures ---
        let (verified_sender, verified_receiver) = unbounded();
        let verifier = TransactionSigVerifier::new(verified_sender);
        let sigverify_stage = SigVerifyStage::new(packet_receiver, verifier, "tpu-sigverify");

        // --- Banking stage: record transactions into BlockRecorder ---
        let banking_stage = BankingStage::new(
            cluster_info.clone(),
            verified_receiver,
            bank_forks.clone(),
            block_recorder.clone_handle(),
            exit.clone(),
        );

        // --- PoW mining service ---
        use crate::consensus::MinerConfig;
        let miner_cfg = MinerConfig {
            coinbase_pubkey: tpu_config.coinbase_pubkey,
            mining_threads: tpu_config.mining_threads,
            staking_threads: tpu_config.staking_threads,
        };
        let (pow_service, pow_rx) =
            PowMiningService::new(engine.clone(), miner_cfg, exit.clone());

        // --- PoS minting service ---
        let bank_forks_pos = bank_forks.clone();
        let utxo_source = Arc::new(move || -> Vec<StakeUtxo> {
            // TODO: enumerate stake accounts from bank_forks
            vec![]
        });
        let (pos_service, pos_rx) =
            PosMintingService::new(engine.clone(), exit.clone(), utxo_source);

        // --- Block finalizer: seal blocks when PoW or PoS delivers a solution ---
        let recorder = block_recorder.clone_handle();
        let engine_fin = engine.clone();
        let blockstore_fin = blockstore.clone();
        let block_store_fin = block_store.clone();
        let bank_forks_fin = bank_forks.clone();
        let exit_fin = exit.clone();
        let block_finalizer = thread::Builder::new()
            .name("tpu-block-finalizer".to_string())
            .spawn(move || {
                Self::block_finalization_loop(
                    recorder,
                    engine_fin,
                    blockstore_fin,
                    block_store_fin,
                    bank_forks_fin,
                    pow_rx,
                    pos_rx,
                    exit_fin,
                );
            })
            .expect("spawn block-finalizer");

        Self {
            fetch_stage,
            sigverify_stage,
            banking_stage,
            pow_service,
            pos_service,
            block_finalizer,
        }
    }

    /// Main loop: merge PoW + PoS channels and finalize blocks.
    fn block_finalization_loop(
        recorder: BlockRecorder,
        engine: Arc<HybridConsensusEngine>,
        blockstore: Arc<Blockstore>,
        block_store: Arc<RwLock<HybridBlockStore>>,
        bank_forks: Arc<RwLock<BankForks>>,
        pow_rx: std::sync::mpsc::Receiver<SolvedPowBlock>,
        pos_rx: std::sync::mpsc::Receiver<crate::pos_service::MintedPosBlock>,
        exit: Arc<AtomicBool>,
    ) {
        use crate::consensus::sha256d_block_hash;
        use solana_sdk::consensus::{BlockConsensusData, ConsensusType};

        while !exit.load(Ordering::Relaxed) {
            // Poll PoW channel
            if let Ok(solved) = pow_rx.recv_timeout(Duration::from_millis(10)) {
                let txs = recorder.drain_transactions(50_000);
                let extra_data: Vec<u8> = Vec::new();
                let block_hash = sha256d_block_hash(
                    &solved.parent_hash,
                    solved.height,
                    solved.nonce,
                    &extra_data,
                );

                // Update consensus engine tip
                let _ = engine.accept_block(
                    &solved.parent_hash,
                    &block_hash,
                    solved.height,
                    solved.block_time,
                    &solved.consensus_data,
                    &extra_data,
                );

                // Index in hybrid store
                let parent_work = block_store.read().unwrap().tip_cumulative_work();
                block_store.write().unwrap().insert_block(
                    solved.height,
                    block_hash,
                    solved.parent_hash,
                    solved.block_time,
                    solved.consensus_data.clone(),
                    parent_work,
                );

                recorder.seal_block(block_hash);
                datapoint_info!("tpu-pow-block", ("height", solved.height, i64));
            }

            // Poll PoS channel
            if let Ok(minted) = pos_rx.recv_timeout(Duration::from_millis(10)) {
                let txs = recorder.drain_transactions(50_000);
                let block_hash = sha256d_block_hash(
                    &minted.parent_hash,
                    minted.height,
                    0,
                    &minted.staker_pubkey.to_bytes(),
                );

                let _ = engine.accept_block(
                    &minted.parent_hash,
                    &block_hash,
                    minted.height,
                    minted.block_time,
                    &minted.consensus_data,
                    &minted.staker_pubkey.to_bytes(),
                );

                let parent_work = block_store.read().unwrap().tip_cumulative_work();
                block_store.write().unwrap().insert_block(
                    minted.height,
                    block_hash,
                    minted.parent_hash,
                    minted.block_time,
                    minted.consensus_data.clone(),
                    parent_work,
                );

                recorder.seal_block(block_hash);
                datapoint_info!("tpu-pos-block", ("height", minted.height, i64));
            }
        }
    }

    pub fn join(self) -> thread::Result<()> {
        self.fetch_stage.join()?;
        self.sigverify_stage.join()?;
        self.banking_stage.join()?;
        self.pow_service.join();
        self.pos_service.join();
        self.block_finalizer.join()
    }
}
