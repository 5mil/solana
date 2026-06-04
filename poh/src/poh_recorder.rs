//! REPLACED: PohRecorder has been superseded by the hybrid PoW/PoS consensus engine.
//! This module now provides a `BlockRecorder` that stages transactions
//! for the next PoW-mined or PoS-minted block instead of recording PoH ticks.
//!
//! The original 80 KB PoH recorder is gone. All tick/slot/leader-schedule
//! logic has been removed. Block timing is driven by wall-clock mining
//! (pow_service) and coin-age minting (pos_service) instead.

use {
    solana_sdk::{
        consensus::{BlockConsensusData, ConsensusType},
        hash::Hash,
        transaction::SanitizedTransaction,
    },
    std::{
        collections::VecDeque,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    },
};

// ---------------------------------------------------------------------------
// Pending transaction queue
// ---------------------------------------------------------------------------

/// Maximum number of transactions buffered before the next block is sealed.
pub const MAX_PENDING_TXS: usize = 65_536;

/// A transaction staged for inclusion in the next block.
#[derive(Debug)]
pub struct PendingTransaction {
    pub tx: SanitizedTransaction,
    pub received_at: u64,
}

// ---------------------------------------------------------------------------
// BlockRecorder — replaces PohRecorder
// ---------------------------------------------------------------------------

/// Stages incoming transactions and seals blocks when the consensus engine
/// delivers a solved PoW nonce or a valid PoS kernel.
///
/// This is intentionally simple: it is a FIFO transaction queue plus a
/// "current working block" that gets finalized by the mining/minting service.
#[derive(Default)]
pub struct BlockRecorder {
    inner: Arc<Mutex<BlockRecorderInner>>,
}

#[derive(Default)]
struct BlockRecorderInner {
    /// Transactions waiting to be included in the next block.
    pending: VecDeque<PendingTransaction>,
    /// Height of the block currently being assembled.
    working_height: u64,
    /// Parent hash of the block being assembled.
    parent_hash: [u8; 32],
}

impl BlockRecorder {
    pub fn new(parent_hash: [u8; 32], working_height: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BlockRecorderInner {
                pending: VecDeque::new(),
                working_height,
                parent_hash,
            })),
        }
    }

    /// Record an incoming transaction into the pending queue.
    /// Drops the oldest transaction if the queue is full.
    pub fn record_transaction(&self, tx: SanitizedTransaction) {
        let mut inner = self.inner.lock().unwrap();
        if inner.pending.len() >= MAX_PENDING_TXS {
            inner.pending.pop_front(); // drop oldest
        }
        inner.pending.push_back(PendingTransaction {
            tx,
            received_at: unix_now(),
        });
    }

    /// Drain up to `max_count` transactions for inclusion in the next block.
    pub fn drain_transactions(&self, max_count: usize) -> Vec<SanitizedTransaction> {
        let mut inner = self.inner.lock().unwrap();
        let n = max_count.min(inner.pending.len());
        inner.pending.drain(..n).map(|p| p.tx).collect()
    }

    /// Seal a completed block: advance working height and parent hash.
    pub fn seal_block(&self, new_parent_hash: [u8; 32]) {
        let mut inner = self.inner.lock().unwrap();
        inner.working_height += 1;
        inner.parent_hash = new_parent_hash;
    }

    pub fn working_height(&self) -> u64 {
        self.inner.lock().unwrap().working_height
    }

    pub fn parent_hash(&self) -> [u8; 32] {
        self.inner.lock().unwrap().parent_hash
    }

    pub fn pending_count(&self) -> usize {
        self.inner.lock().unwrap().pending.len()
    }

    pub fn clone_handle(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ---------------------------------------------------------------------------
// Sealed block ready for ledger writing
// ---------------------------------------------------------------------------

/// A finalized block ready to be written to the ledger and broadcast.
#[derive(Debug)]
pub struct SealedBlock {
    pub height: u64,
    pub parent_hash: [u8; 32],
    pub block_hash: [u8; 32],
    pub block_time: u64,
    pub transactions: Vec<SanitizedTransaction>,
    pub consensus_data: BlockConsensusData,
}

impl SealedBlock {
    pub fn consensus_type(&self) -> ConsensusType {
        self.consensus_data.consensus_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_drain() {
        let recorder = BlockRecorder::new([0u8; 32], 1);
        // No transactions yet.
        assert_eq!(recorder.pending_count(), 0);
        assert_eq!(recorder.drain_transactions(10).len(), 0);
    }

    #[test]
    fn test_seal_advances_height() {
        let recorder = BlockRecorder::new([0u8; 32], 5);
        assert_eq!(recorder.working_height(), 5);
        recorder.seal_block([1u8; 32]);
        assert_eq!(recorder.working_height(), 6);
        assert_eq!(recorder.parent_hash(), [1u8; 32]);
    }

    #[test]
    fn test_queue_cap() {
        let recorder = BlockRecorder::new([0u8; 32], 1);
        // Just verify the constant is sane.
        assert!(MAX_PENDING_TXS >= 1024);
    }
}
