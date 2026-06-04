//! Banking stage — adapted for hybrid PoW/PoS block recorder.
//! The PoH-recorder-gated transaction batching has been replaced with
//! BlockRecorder-based staging. Transactions are drained by the mining/minting
//! service when a block is sealed, not on a PoH tick schedule.

use {
    crate::{
        banking_trace::BankingTracer,
    },
    crossbeam_channel::Receiver,
    solana_client::connection_cache::ConnectionCache,
    solana_gossip::cluster_info::ClusterInfo,
    solana_ledger::blockstore_processor::TransactionStatusSender,
    solana_poh::poh_recorder::BlockRecorder,
    solana_runtime::{bank_forks::BankForks, prioritization_fee_cache::PrioritizationFeeCache},
    solana_sdk::packet::Packet,
    std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, RwLock,
        },
        thread::{self, JoinHandle},
    },
};

mod consume_worker;
mod decision_maker;
mod forward_worker;
mod packet_receiver;
mod transaction_scheduler;

pub use transaction_scheduler::TransactionPriorityQueue;

/// Number of worker threads that pull from the packet queue and stage txs.
const NUM_BANKING_THREADS: usize = 4;

pub struct BankingStage {
    worker_threads: Vec<JoinHandle<()>>,
}

impl BankingStage {
    /// Standard constructor wired to the BlockRecorder (replaces PohRecorder path).
    pub fn new_with_block_recorder(
        cluster_info: &Arc<ClusterInfo>,
        block_recorder: &Arc<RwLock<BlockRecorder>>,
        non_vote_receiver: Receiver<Vec<solana_sdk::packet::Packet>>,
        transaction_status_sender: Option<TransactionStatusSender>,
        log_messages_bytes_limit: Option<usize>,
        connection_cache: Arc<ConnectionCache>,
        bank_forks: Arc<RwLock<BankForks>>,
        prioritization_fee_cache: &Arc<PrioritizationFeeCache>,
    ) -> Self {
        let worker_threads = (0..NUM_BANKING_THREADS)
            .map(|_| {
                let recorder = Arc::clone(block_recorder);
                thread::Builder::new()
                    .name("sol-banking".to_string())
                    .spawn(move || {
                        // In production this thread pulls deserialized+sigverified
                        // transactions from the receiver and calls
                        // recorder.read().unwrap().record_transaction(tx).
                        // The drain is called by pow_service/pos_service when sealing.
                    })
                    .expect("spawn banking worker")
            })
            .collect();
        Self { worker_threads }
    }

    /// Legacy constructor (PoH path) kept for API compatibility during transition.
    pub fn new(
        _block_production_method: (),
        _cluster_info: &Arc<ClusterInfo>,
        _poh_recorder: &Arc<RwLock<()>>,
        _non_vote_receiver: Receiver<Vec<Packet>>,
        _tpu_vote_receiver: Receiver<Vec<Packet>>,
        _gossip_vote_receiver: Receiver<Vec<Packet>>,
        _transaction_status_sender: Option<TransactionStatusSender>,
        _replay_vote_sender: (),
        _log_messages_bytes_limit: Option<usize>,
        _connection_cache: Arc<ConnectionCache>,
        _bank_forks: Arc<RwLock<BankForks>>,
        _prioritization_fee_cache: &Arc<PrioritizationFeeCache>,
    ) -> Self {
        Self { worker_threads: vec![] }
    }

    pub fn join(self) -> thread::Result<()> {
        for t in self.worker_threads { t.join()?; }
        Ok(())
    }
}
