//! Validator startup — rewritten for hybrid PoW/PoS consensus.
//! PoH recorder/service startup removed. HybridConsensusEngine bootstrapped here.

use {
    solana_core::{
        consensus::{HybridConsensusEngine, MinerConfig},
        replay_stage::ReplayStage,
        tpu::{Tpu, TpuSockets},
        banking_trace::BankingTracer,
        pos_service::StakeUtxo,
    },
    solana_gossip::cluster_info::ClusterInfo,
    solana_ledger::{
        blockstore::Blockstore,
        hybrid_block_store::HybridBlockStore,
    },
    solana_poh::poh_recorder::BlockRecorder,
    solana_runtime::{
        bank_forks::BankForks,
        genesis_utils::create_genesis_config,
    },
    solana_sdk::{
        consensus::{HybridConsensusConfig, PowConfig, PosConfig},
        genesis_config::GenesisConfig,
        pubkey::Pubkey,
        signature::Keypair,
    },
    std::{
        net::SocketAddr,
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
            Arc, RwLock,
        },
        thread,
        time::Duration,
    },
};

/// Top-level validator configuration for the hybrid chain.
pub struct ValidatorConfig {
    pub genesis_config: GenesisConfig,
    pub consensus_config: HybridConsensusConfig,
    pub miner_config: MinerConfig,
    pub mining_enabled: bool,
    pub staking_enabled: bool,
    pub node_keypair: Arc<Keypair>,
    pub ledger_path: std::path::PathBuf,
    pub rpc_addr: Option<SocketAddr>,
    pub max_peers: usize,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            genesis_config: GenesisConfig::default(),
            consensus_config: HybridConsensusConfig::default(),
            miner_config: MinerConfig::default(),
            mining_enabled: true,
            staking_enabled: true,
            node_keypair: Arc::new(Keypair::new()),
            ledger_path: std::path::PathBuf::from("ledger"),
            rpc_addr: None,
            max_peers: 125,
        }
    }
}

/// Running validator instance.
pub struct Validator {
    exit: Arc<AtomicBool>,
    replay_stage: Option<ReplayStage>,
}

impl Validator {
    pub fn new(
        config: ValidatorConfig,
        cluster_info: Arc<ClusterInfo>,
        bank_forks: Arc<RwLock<BankForks>>,
        blockstore: Arc<Blockstore>,
    ) -> Self {
        let exit = Arc::new(AtomicBool::new(false));

        // 1. Bootstrap consensus engine.
        let consensus_engine = Arc::new(HybridConsensusEngine::new(
            config.consensus_config.clone(),
            Arc::clone(&exit),
        ));

        // 2. Bootstrap block recorder (replaces PohRecorder).
        let genesis_hash = [0u8; 32]; // loaded from genesis file in production
        let block_recorder = Arc::new(RwLock::new(
            BlockRecorder::new(genesis_hash, 1),
        ));

        // 3. Sealed block channel: mining/minting -> replay stage.
        let (sealed_block_tx, sealed_block_rx) = mpsc::channel();

        // 4. Replay stage: validates & commits blocks.
        let genesis_config = Arc::new(config.genesis_config.clone());
        let replay_stage = ReplayStage::new(
            Arc::clone(&genesis_config),
            Arc::clone(&consensus_engine),
            Arc::clone(&bank_forks),
            Arc::clone(&blockstore),
            sealed_block_rx,
            Arc::clone(&exit),
        );

        info!(
            "Validator started: mining={} staking={} threads={}",
            config.mining_enabled,
            config.staking_enabled,
            config.miner_config.mining_threads
        );

        Self {
            exit,
            replay_stage: Some(replay_stage),
        }
    }

    pub fn close(mut self) {
        self.exit.store(true, Ordering::Relaxed);
        if let Some(stage) = self.replay_stage.take() {
            let _ = stage.join();
        }
    }
}
