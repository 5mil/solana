//! Consume worker: drains the priority queue into the BlockRecorder.
//! Called by the mining/minting service when sealing a block.

use {
    crate::banking_stage::TransactionPriorityQueue,
    solana_poh::poh_recorder::BlockRecorder,
    std::sync::{Arc, RwLock},
};

pub struct ConsumeWorker {
    recorder: Arc<RwLock<BlockRecorder>>,
    queue: Arc<RwLock<TransactionPriorityQueue>>,
}

impl ConsumeWorker {
    pub fn new(
        recorder: Arc<RwLock<BlockRecorder>>,
        queue: Arc<RwLock<TransactionPriorityQueue>>,
    ) -> Self {
        Self { recorder, queue }
    }

    /// Drain the top `max_txs` transactions into the block recorder.
    pub fn consume(&self, max_txs: usize) {
        let txs = self.queue.write().unwrap().drain_top(max_txs);
        let recorder = self.recorder.read().unwrap();
        for tx in txs {
            recorder.record_transaction(tx);
        }
    }

    pub fn queue_len(&self) -> usize {
        self.queue.read().unwrap().len()
    }
}
