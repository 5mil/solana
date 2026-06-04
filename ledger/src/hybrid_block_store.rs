//! Hybrid block store — extends the ledger with PoW/PoS consensus metadata.
//!
//! The existing `Blockstore` (rocksdb-backed) is untouched. This module adds
//! column-family helpers and index structures specific to the hybrid chain:
//!   - Per-block consensus metadata (type, nonce, difficulty, stake modifier)
//!   - Difficulty index: maps height -> difficulty_bits for retargeting queries
//!   - PoW chain-work index: cumulative work per block for fork choice
//!   - PoS spent coin-age index: prevents reuse of coin age across blocks

use {
    solana_sdk::{
        consensus::{BlockConsensusData, ConsensusType},
        genesis_config::GenesisConfig,
    },
    std::collections::BTreeMap,
};

// ---------------------------------------------------------------------------
// In-memory consensus metadata index
// ---------------------------------------------------------------------------
// NOTE: Production will replace this with RocksDB column families.
// This in-memory implementation is sufficient for the initial integration.

/// Consensus metadata stored per block height.
#[derive(Clone, Debug)]
pub struct BlockConsensusRecord {
    pub height: u64,
    pub block_hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub block_time: u64,
    pub consensus_data: BlockConsensusData,
    /// Cumulative PoW work at this block (sum of 2^difficulty_bits for all prior PoW blocks).
    pub cumulative_work: u128,
}

/// The hybrid block store index layered on top of the ledger.
pub struct HybridBlockStore {
    /// Height -> consensus record.
    by_height: BTreeMap<u64, BlockConsensusRecord>,
    /// Block hash -> height (for quick lookup by hash).
    by_hash: BTreeMap<[u8; 32], u64>,
    /// Set of (txout_hash, txout_n) pairs whose coin-age has been consumed.
    /// Prevents double-minting in PoS.
    spent_coin_age: std::collections::HashSet<([u8; 32], u32)>,
}

impl Default for HybridBlockStore {
    fn default() -> Self {
        Self {
            by_height: BTreeMap::new(),
            by_hash: BTreeMap::new(),
            spent_coin_age: std::collections::HashSet::new(),
        }
    }
}

impl HybridBlockStore {
    pub fn new() -> Self {
        Self::default()
    }

    // -----------------------------------------------------------------------
    // Write path
    // -----------------------------------------------------------------------

    /// Record a newly accepted block.
    pub fn insert_block(
        &mut self,
        height: u64,
        block_hash: [u8; 32],
        parent_hash: [u8; 32],
        block_time: u64,
        consensus_data: BlockConsensusData,
        parent_cumulative_work: u128,
    ) {
        let block_work: u128 = if consensus_data.consensus_type == ConsensusType::ProofOfWork {
            1u128 << consensus_data.difficulty_bits.min(127)
        } else {
            0
        };
        let cumulative_work = parent_cumulative_work.saturating_add(block_work);

        self.by_height.insert(height, BlockConsensusRecord {
            height,
            block_hash,
            parent_hash,
            block_time,
            consensus_data,
            cumulative_work,
        });
        self.by_hash.insert(block_hash, height);
    }

    /// Mark a coin-age UTXO as spent (consumed in a PoS mint).
    pub fn mark_coin_age_spent(&mut self, txout_hash: [u8; 32], txout_n: u32) {
        self.spent_coin_age.insert((txout_hash, txout_n));
    }

    // -----------------------------------------------------------------------
    // Read path
    // -----------------------------------------------------------------------

    pub fn get_by_height(&self, height: u64) -> Option<&BlockConsensusRecord> {
        self.by_height.get(&height)
    }

    pub fn get_by_hash(&self, hash: &[u8; 32]) -> Option<&BlockConsensusRecord> {
        self.by_hash.get(hash).and_then(|h| self.by_height.get(h))
    }

    /// Current best-tip height.
    pub fn tip_height(&self) -> u64 {
        self.by_height.keys().next_back().copied().unwrap_or(0)
    }

    /// Cumulative PoW work at the current tip.
    pub fn tip_cumulative_work(&self) -> u128 {
        self.by_height
            .values()
            .next_back()
            .map(|r| r.cumulative_work)
            .unwrap_or(0)
    }

    /// Returns true if the UTXO's coin-age has already been consumed.
    pub fn is_coin_age_spent(&self, txout_hash: &[u8; 32], txout_n: u32) -> bool {
        self.spent_coin_age.contains(&(*txout_hash, txout_n))
    }

    // -----------------------------------------------------------------------
    // Difficulty queries (used by retargeting)
    // -----------------------------------------------------------------------

    /// Get the difficulty bits at a specific block height.
    pub fn difficulty_at(&self, height: u64) -> Option<u32> {
        self.by_height.get(&height).map(|r| r.consensus_data.difficulty_bits)
    }

    /// Find the actual timespan of the last `window` PoW blocks ending at `tip_height`.
    /// Returns `None` if there are not enough PoW blocks yet.
    pub fn pow_window_timespan(&self, tip_height: u64, window: u64) -> Option<u64> {
        let tip = self.by_height.get(&tip_height)?;

        // Walk back `window` PoW blocks.
        let mut pow_count = 0u64;
        let mut window_start_time = tip.block_time;
        let mut h = tip_height;

        while pow_count < window && h > 0 {
            h -= 1;
            if let Some(rec) = self.by_height.get(&h) {
                if rec.consensus_data.consensus_type == ConsensusType::ProofOfWork {
                    window_start_time = rec.block_time;
                    pow_count += 1;
                }
            }
        }

        if pow_count < window {
            None // not enough history
        } else {
            Some(tip.block_time.saturating_sub(window_start_time))
        }
    }

    // -----------------------------------------------------------------------
    // Fork choice
    // -----------------------------------------------------------------------

    /// Given two tip hashes, return the hash of the heavier chain.
    pub fn heavier_tip(
        &self,
        hash_a: &[u8; 32],
        hash_b: &[u8; 32],
    ) -> Option<[u8; 32]> {
        let work_a = self.get_by_hash(hash_a).map(|r| r.cumulative_work).unwrap_or(0);
        let work_b = self.get_by_hash(hash_b).map(|r| r.cumulative_work).unwrap_or(0);
        if work_a >= work_b {
            Some(*hash_a)
        } else {
            Some(*hash_b)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::consensus::{BlockConsensusData, ConsensusType};

    fn pow_record(height: u64, bits: u32) -> BlockConsensusData {
        BlockConsensusData {
            consensus_type: ConsensusType::ProofOfWork,
            nonce: 0,
            difficulty_bits: bits,
            stake_modifier: [0u8; 32],
            coin_age_consumed: 0,
        }
    }

    #[test]
    fn test_insert_and_lookup() {
        let mut store = HybridBlockStore::new();
        let hash = [1u8; 32];
        store.insert_block(1, hash, [0u8; 32], 1000, pow_record(1, 20), 0);
        let rec = store.get_by_height(1).unwrap();
        assert_eq!(rec.height, 1);
        assert_eq!(rec.cumulative_work, 1u128 << 20);
    }

    #[test]
    fn test_heavier_chain_selection() {
        let mut store = HybridBlockStore::new();
        let hash_a = [1u8; 32];
        let hash_b = [2u8; 32];
        store.insert_block(1, hash_a, [0u8; 32], 1000, pow_record(1, 20), 0);
        store.insert_block(1, hash_b, [0u8; 32], 1000, pow_record(1, 24), 0); // higher work
        let winner = store.heavier_tip(&hash_a, &hash_b).unwrap();
        assert_eq!(winner, hash_b);
    }

    #[test]
    fn test_coin_age_spent() {
        let mut store = HybridBlockStore::new();
        let txhash = [9u8; 32];
        assert!(!store.is_coin_age_spent(&txhash, 0));
        store.mark_coin_age_spent(txhash, 0);
        assert!(store.is_coin_age_spent(&txhash, 0));
    }
}
