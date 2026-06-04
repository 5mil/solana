//! Transaction Validation Unit — modified for hybrid PoW/PoS consensus.
//!
//! The TVU now validates incoming blocks against the consensus engine
//! (PoW hash check or PoS coin-age check) before replaying them.
//! Leader-schedule-based vote filtering has been removed.

use {
    crate::{
        cluster_slots_service::ClusterSlotsService,
        completed_data_sets_service::CompletedDataSetsService,
        consensus::HybridConsensusEngine,
        repair::RepairService,
        replay_stage::{ReplayStage, ReplayStageConfig},
        shred_fetch_stage::ShredFetchStage,
        window_service::WindowService,
    },
    solana_gossip::cluster_info::ClusterInfo,
    solana_ledger::{
        blockstore::Blockstore,
        hybrid_block_store::HybridBlockStore,
        blockstore_processor::ProcessBlockStore,
    },
    solana_runtime::{
        bank_forks::BankForks,
        block_validator::validate_block_consensus,
    },
    solana_sdk::{
        consensus::BlockConsensusData,
        genesis_config::GenesisConfig,
    },
    std::{
        net::UdpSocket,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, RwLock,
        },
        thread::JoinHandle,
    },
};

/// All join handles owned by the TVU.
pub struct Tvu {
    shred_fetch_stage: ShredFetchStage,
    window_service: WindowService,
    cluster_slots_service: ClusterSlotsService,
    replay_stage: ReplayStage,
    completed_data_sets_service: CompletedDataSetsService,
}

/// Configuration for the hybrid consensus TVU.
pub struct TvuConfig {
    pub max_ledger_shreds: Option<u64>,
    pub shred_version: u16,
}

impl Default for TvuConfig {
    fn default() -> Self {
        Self {
            max_ledger_shreds: None,
            shred_version: 0,
        }
    }
}

impl Tvu {
    pub fn new(
        cluster_info: Arc<ClusterInfo>,
        bank_forks: Arc<RwLock<BankForks>>,
        blockstore: Arc<Blockstore>,
        block_store: Arc<RwLock<HybridBlockStore>>,
        engine: Arc<HybridConsensusEngine>,
        genesis_config: Arc<GenesisConfig>,
        sockets: Vec<UdpSocket>,
        tvu_config: TvuConfig,
        exit: Arc<AtomicBool>,
    ) -> Self {
        let (shred_fetch_sender, shred_fetch_receiver) =
            crossbeam_channel::unbounded();

        let shred_fetch_stage = ShredFetchStage::new(
            sockets,
            shred_fetch_sender,
            tvu_config.shred_version,
            exit.clone(),
        );

        let (completed_data_sets_sender, completed_data_sets_receiver) =
            crossbeam_channel::unbounded();

        let window_service = WindowService::new(
            blockstore.clone(),
            cluster_info.clone(),
            shred_fetch_receiver,
            completed_data_sets_sender,
            exit.clone(),
        );

        let cluster_slots_service =
            ClusterSlotsService::new(cluster_info.clone(), bank_forks.clone(), exit.clone());

        let replay_config = ReplayStageConfig {
            block_store: block_store.clone(),
            engine: engine.clone(),
            genesis_config: genesis_config.clone(),
        };
        let replay_stage = ReplayStage::new(
            replay_config,
            blockstore.clone(),
            bank_forks.clone(),
            exit.clone(),
        );

        let completed_data_sets_service = CompletedDataSetsService::new(
            completed_data_sets_receiver,
            blockstore.clone(),
            exit.clone(),
        );

        Self {
            shred_fetch_stage,
            window_service,
            cluster_slots_service,
            replay_stage,
            completed_data_sets_service,
        }
    }

    pub fn join(self) -> std::thread::Result<()> {
        self.shred_fetch_stage.join()?;
        self.window_service.join()?;
        self.cluster_slots_service.join()?;
        self.replay_stage.join()?;
        self.completed_data_sets_service.join()
    }
}
