//! Transaction Processing Unit — modified for hybrid PoW/PoS consensus.
//! PohRecorder references have been replaced with BlockRecorder.
//! Leader-schedule and vote-listener wiring removed from block-production path.

pub use solana_sdk::net::DEFAULT_TPU_COALESCE;
use {
    crate::{
        banking_stage::BankingStage,
        banking_trace::{BankingTracer, TracerThread},
        fetch_stage::FetchStage,
        sigverify::TransactionSigVerifier,
        sigverify_stage::SigVerifyStage,
        staked_nodes_updater_service::StakedNodesUpdaterService,
        pow_service::{PowMiningService, SolvedPowBlock},
        pos_service::{PosMintingService, MintedPosBlock, StakeUtxo},
        consensus::{HybridConsensusEngine, MinerConfig},
    },
    bytes::Bytes,
    crossbeam_channel::{unbounded, Receiver},
    solana_client::connection_cache::ConnectionCache,
    solana_gossip::cluster_info::ClusterInfo,
    solana_ledger::{
        blockstore::Blockstore,
        blockstore_processor::TransactionStatusSender,
    },
    solana_poh::poh_recorder::BlockRecorder,
    solana_rpc::{
        optimistically_confirmed_bank_tracker::BankNotificationSender,
        rpc_subscriptions::RpcSubscriptions,
    },
    solana_runtime::{bank_forks::BankForks, prioritization_fee_cache::PrioritizationFeeCache},
    solana_sdk::{
        consensus::HybridConsensusConfig,
        pubkey::Pubkey,
        quic::NotifyKeyUpdate,
        signature::Keypair,
    },
    solana_streamer::{
        nonblocking::quic::DEFAULT_WAIT_FOR_CHUNK_TIMEOUT,
        quic::{spawn_server, SpawnServerResult, MAX_STAKED_CONNECTIONS, MAX_UNSTAKED_CONNECTIONS},
        streamer::StakedNodes,
    },
    solana_turbine::broadcast_stage::{BroadcastStage, BroadcastStageType},
    std::{
        collections::HashMap,
        net::{SocketAddr, UdpSocket},
        sync::{atomic::AtomicBool, Arc, RwLock},
        thread,
        time::Duration,
    },
    tokio::sync::mpsc::Sender as AsyncSender,
};

pub const MAX_QUIC_CONNECTIONS_PER_PEER: usize = 8;

pub struct TpuSockets {
    pub transactions: Vec<UdpSocket>,
    pub transaction_forwards: Vec<UdpSocket>,
    pub vote: Vec<UdpSocket>,
    pub broadcast: Vec<UdpSocket>,
    pub transactions_quic: UdpSocket,
    pub transactions_forwards_quic: UdpSocket,
}

pub struct Tpu {
    fetch_stage: FetchStage,
    sigverify_stage: SigVerifyStage,
    banking_stage: BankingStage,
    broadcast_stage: BroadcastStage,
    tpu_quic_t: thread::JoinHandle<()>,
    tpu_forwards_quic_t: thread::JoinHandle<()>,
    staked_nodes_updater_service: StakedNodesUpdaterService,
    tracer_thread_hdl: TracerThread,
    pow_mining_service: Option<PowMiningService>,
    pos_minting_service: Option<PosMintingService>,
}

impl Tpu {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cluster_info: &Arc<ClusterInfo>,
        block_recorder: &Arc<RwLock<BlockRecorder>>,
        sockets: TpuSockets,
        subscriptions: &Arc<RpcSubscriptions>,
        transaction_status_sender: Option<TransactionStatusSender>,
        blockstore: Arc<Blockstore>,
        broadcast_type: &BroadcastStageType,
        exit: Arc<AtomicBool>,
        shred_version: u16,
        bank_forks: Arc<RwLock<BankForks>>,
        bank_notification_sender: Option<BankNotificationSender>,
        tpu_coalesce: Duration,
        connection_cache: &Arc<ConnectionCache>,
        turbine_quic_endpoint_sender: AsyncSender<(SocketAddr, Bytes)>,
        keypair: &Keypair,
        log_messages_bytes_limit: Option<usize>,
        staked_nodes: &Arc<RwLock<StakedNodes>>,
        shared_staked_nodes_overrides: Arc<RwLock<HashMap<Pubkey, u64>>>,
        banking_tracer: Arc<BankingTracer>,
        tracer_thread_hdl: TracerThread,
        tpu_enable_udp: bool,
        prioritization_fee_cache: &Arc<PrioritizationFeeCache>,
        // Hybrid consensus
        consensus_engine: Arc<HybridConsensusEngine>,
        miner_config: MinerConfig,
        utxo_source: Arc<dyn Fn() -> Vec<StakeUtxo> + Send + Sync + 'static>,
    ) -> (Self, Vec<Arc<dyn NotifyKeyUpdate + Sync + Send>>) {
        let TpuSockets {
            transactions: transactions_sockets,
            transaction_forwards: tpu_forwards_sockets,
            vote: tpu_vote_sockets,
            broadcast: broadcast_sockets,
            transactions_quic: transactions_quic_sockets,
            transactions_forwards_quic: transactions_forwards_quic_sockets,
        } = sockets;

        let (packet_sender, packet_receiver) = unbounded();
        let (forwarded_packet_sender, forwarded_packet_receiver) = unbounded();

        let fetch_stage = FetchStage::new_with_sender(
            transactions_sockets,
            tpu_forwards_sockets,
            tpu_vote_sockets,
            exit.clone(),
            &packet_sender,
            &packet_sender.clone(),
            &forwarded_packet_sender,
            forwarded_packet_receiver,
            tpu_coalesce,
            tpu_enable_udp,
        );

        let staked_nodes_updater_service = StakedNodesUpdaterService::new(
            exit.clone(),
            bank_forks.clone(),
            staked_nodes.clone(),
            shared_staked_nodes_overrides,
        );

        let (non_vote_sender, non_vote_receiver) = banking_tracer.create_channel_non_vote();

        let SpawnServerResult {
            endpoint: _,
            thread: tpu_quic_t,
            key_updater,
        } = spawn_server(
            "solQuicTpu",
            "quic_streamer_tpu",
            transactions_quic_sockets,
            keypair,
            packet_sender,
            exit.clone(),
            MAX_QUIC_CONNECTIONS_PER_PEER,
            staked_nodes.clone(),
            MAX_STAKED_CONNECTIONS,
            MAX_UNSTAKED_CONNECTIONS,
            DEFAULT_WAIT_FOR_CHUNK_TIMEOUT,
            tpu_coalesce,
        )
        .unwrap();

        let SpawnServerResult {
            endpoint: _,
            thread: tpu_forwards_quic_t,
            key_updater: forwards_key_updater,
        } = spawn_server(
            "solQuicTpuFwd",
            "quic_streamer_tpu_forwards",
            transactions_forwards_quic_sockets,
            keypair,
            forwarded_packet_sender,
            exit.clone(),
            MAX_QUIC_CONNECTIONS_PER_PEER,
            staked_nodes.clone(),
            MAX_STAKED_CONNECTIONS.saturating_add(MAX_UNSTAKED_CONNECTIONS),
            0,
            DEFAULT_WAIT_FOR_CHUNK_TIMEOUT,
            tpu_coalesce,
        )
        .unwrap();

        let sigverify_stage = {
            let verifier = TransactionSigVerifier::new(non_vote_sender);
            SigVerifyStage::new(packet_receiver, verifier, "solSigVerTpu", "tpu-verifier")
        };

        let banking_stage = BankingStage::new_with_block_recorder(
            cluster_info,
            block_recorder,
            non_vote_receiver,
            transaction_status_sender,
            log_messages_bytes_limit,
            connection_cache.clone(),
            bank_forks.clone(),
            prioritization_fee_cache,
        );

        // Spawn PoW mining service.
        let (pow_mining_service, _pow_rx) =
            PowMiningService::new(Arc::clone(&consensus_engine), miner_config, exit.clone());

        // Spawn PoS minting service.
        let (pos_minting_service, _pos_rx) =
            PosMintingService::new(Arc::clone(&consensus_engine), exit.clone(), utxo_source);

        // Broadcast stage: takes sealed blocks from the block recorder.
        let broadcast_stage = broadcast_type.new_broadcast_stage(
            broadcast_sockets,
            cluster_info.clone(),
            exit.clone(),
            blockstore,
            bank_forks,
            shred_version,
            turbine_quic_endpoint_sender,
        );

        (
            Self {
                fetch_stage,
                sigverify_stage,
                banking_stage,
                broadcast_stage,
                tpu_quic_t,
                tpu_forwards_quic_t,
                staked_nodes_updater_service,
                tracer_thread_hdl,
                pow_mining_service: Some(pow_mining_service),
                pos_minting_service: Some(pos_minting_service),
            },
            vec![key_updater, forwards_key_updater],
        )
    }

    pub fn join(self) -> thread::Result<()> {
        self.fetch_stage.join()?;
        self.sigverify_stage.join()?;
        self.banking_stage.join()?;
        self.staked_nodes_updater_service.join()?;
        self.tpu_quic_t.join()?;
        self.tpu_forwards_quic_t.join()?;
        if let Some(s) = self.pow_mining_service { s.join(); }
        if let Some(s) = self.pos_minting_service { s.join(); }
        let _ = self.broadcast_stage.join();
        if let Some(tracer_thread_hdl) = self.tracer_thread_hdl {
            if let Err(e) = tracer_thread_hdl.join()? {
                error!("banking tracer thread error: {:?}", e);
            }
        }
        Ok(())
    }
}
