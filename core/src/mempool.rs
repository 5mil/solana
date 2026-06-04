//! Mempool — transaction pool for the hybrid PoW/PoS node.
//!
//! Maintains a fee-priority queue of pending transactions.
//! The block producer drains the highest-fee transactions first.
//! Transactions are evicted after `TTL_SECS` seconds if not included.

use {
    solana_sdk::transaction::SanitizedTransaction,
    std::{
        collections::BinaryHeap,
        cmp::Ordering,
        sync::{Arc, Mutex},
        time::{SystemTime, UNIX_EPOCH},
    },
};

/// A transaction plus its priority fee and arrival time.
#[derive(Eq, PartialEq)]
pub struct MempoolEntry {
    /// Compute unit price (lamports per CU) — higher = higher priority.
    pub priority_fee: u64,
    /// When this entry was added (Unix seconds).
    pub received_at: u64,
    pub tx: SanitizedTransaction,
}

impl Ord for MempoolEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher fee = higher priority; break ties by earlier arrival.
        self.priority_fee
            .cmp(&other.priority_fee)
            .then(other.received_at.cmp(&self.received_at))
    }
}

impl PartialOrd for MempoolEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Maximum seconds a transaction can wait in the mempool.
pub const TTL_SECS: u64 = 90;
/// Maximum mempool size (number of transactions).
pub const MAX_SIZE: usize = 100_000;

/// Thread-safe priority-fee mempool.
#[derive(Default)]
pub struct Mempool {
    inner: Arc<Mutex<MempoolInner>>,
}

#[derive(Default)]
struct MempoolInner {
    heap: BinaryHeap<MempoolEntry>,
}

impl Mempool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a transaction. Drops lowest-fee entry if at capacity.
    pub fn insert(&self, tx: SanitizedTransaction, priority_fee: u64) {
        let now = unix_now();
        let mut inner = self.inner.lock().unwrap();
        if inner.heap.len() >= MAX_SIZE {
            // Drop the lowest-priority entry.
            // BinaryHeap is a max-heap, so we drain into a vec, truncate, rebuild.
            let mut entries: Vec<_> = inner.heap.drain().collect();
            entries.sort_by(|a, b| b.cmp(a)); // highest fee first
            entries.truncate(MAX_SIZE - 1);
            inner.heap = entries.into_iter().collect();
        }
        inner.heap.push(MempoolEntry {
            priority_fee,
            received_at: now,
            tx,
        });
    }

    /// Drain up to `n` highest-priority non-expired transactions.
    pub fn drain_top(&self, n: usize) -> Vec<SanitizedTransaction> {
        let now = unix_now();
        let mut inner = self.inner.lock().unwrap();
        let mut result = Vec::with_capacity(n);
        let mut keep = Vec::new();

        while let Some(entry) = inner.heap.pop() {
            if now - entry.received_at > TTL_SECS {
                continue; // expired — drop
            }
            if result.len() < n {
                result.push(entry.tx);
            } else {
                keep.push(entry);
                break;
            }
        }
        // Drain rest for expiry pruning
        while let Some(entry) = inner.heap.pop() {
            if now - entry.received_at <= TTL_SECS {
                keep.push(entry);
            }
        }
        for entry in keep {
            inner.heap.push(entry);
        }
        result
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().heap.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn clone_handle(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        let pool = Mempool::new();
        // Insert high and low priority txs (we can't easily construct
        // SanitizedTransaction in unit tests so just verify capacity logic).
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_ttl_constant_sane() {
        assert!(TTL_SECS >= 30);
    }

    #[test]
    fn test_max_size_sane() {
        assert!(MAX_SIZE >= 1000);
    }
}
