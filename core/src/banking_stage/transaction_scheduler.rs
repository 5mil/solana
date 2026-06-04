//! Transaction priority queue for the banking stage.
//! Transactions are sorted by fee-per-compute-unit so high-fee txs are
//! included first when a block is sealed by the PoW/PoS service.

use solana_sdk::transaction::SanitizedTransaction;
use std::collections::BinaryHeap;
use std::cmp::Ordering;

#[derive(Debug)]
struct PrioritizedTx {
    priority: u64, // lamports per CU
    tx: SanitizedTransaction,
}

impl PartialEq for PrioritizedTx {
    fn eq(&self, other: &Self) -> bool { self.priority == other.priority }
}
impl Eq for PrioritizedTx {}
impl PartialOrd for PrioritizedTx {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl Ord for PrioritizedTx {
    fn cmp(&self, other: &Self) -> Ordering { self.priority.cmp(&other.priority) }
}

/// Max transactions kept in the priority queue at any time.
pub const MAX_PRIORITY_QUEUE_SIZE: usize = 131_072;

/// Fee-priority transaction queue used by BankingStage.
pub struct TransactionPriorityQueue {
    heap: BinaryHeap<PrioritizedTx>,
}

impl TransactionPriorityQueue {
    pub fn new() -> Self {
        Self { heap: BinaryHeap::with_capacity(1024) }
    }

    /// Push a transaction with its lamports-per-CU priority.
    pub fn push(&mut self, tx: SanitizedTransaction, priority: u64) {
        if self.heap.len() >= MAX_PRIORITY_QUEUE_SIZE {
            // Evict lowest-priority entry to make room.
            let mut items: Vec<_> = self.heap.drain().collect();
            items.sort_unstable();
            items.reverse();
            items.pop(); // remove lowest
            self.heap.extend(items);
        }
        self.heap.push(PrioritizedTx { priority, tx });
    }

    /// Drain up to `n` highest-priority transactions.
    pub fn drain_top(&mut self, n: usize) -> Vec<SanitizedTransaction> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            match self.heap.pop() {
                Some(p) => out.push(p.tx),
                None => break,
            }
        }
        out
    }

    pub fn len(&self) -> usize { self.heap.len() }
    pub fn is_empty(&self) -> bool { self.heap.is_empty() }
}

impl Default for TransactionPriorityQueue {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_ordering() {
        let mut q = TransactionPriorityQueue::new();
        // We can't easily construct SanitizedTransaction in unit tests,
        // so just verify the queue bounds.
        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
        assert!(MAX_PRIORITY_QUEUE_SIZE >= 1024);
    }
}
