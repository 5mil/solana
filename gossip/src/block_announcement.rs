//! P2P block announcement for the hybrid PoW/PoS chain.
//! When a node mines or mints a new block it broadcasts a compact
//! `BlockAnnouncement` to all peers via gossip. Peers can then request
//! the full block.

use {
    serde::{Deserialize, Serialize},
    solana_sdk::{
        consensus::{BlockConsensusData, ConsensusType},
        pubkey::Pubkey,
    },
};

/// Compact block announcement broadcast to the gossip network.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockAnnouncement {
    /// Block height.
    pub height: u64,
    /// SHA256d block hash.
    pub block_hash: [u8; 32],
    /// Parent block hash.
    pub parent_hash: [u8; 32],
    /// Unix timestamp.
    pub block_time: u64,
    /// Consensus type (PoW or PoS).
    pub consensus_type: ConsensusType,
    /// Difficulty bits (PoW) or stake modifier (PoS).
    pub difficulty_bits: u32,
    /// Announcer pubkey (the node that produced the block).
    pub producer: Pubkey,
    /// Cumulative chain work at this block.
    pub cumulative_work: u128,
}

impl BlockAnnouncement {
    pub fn new_pow(
        height: u64,
        block_hash: [u8; 32],
        parent_hash: [u8; 32],
        block_time: u64,
        difficulty_bits: u32,
        producer: Pubkey,
        cumulative_work: u128,
    ) -> Self {
        Self {
            height,
            block_hash,
            parent_hash,
            block_time,
            consensus_type: ConsensusType::ProofOfWork,
            difficulty_bits,
            producer,
            cumulative_work,
        }
    }

    pub fn new_pos(
        height: u64,
        block_hash: [u8; 32],
        parent_hash: [u8; 32],
        block_time: u64,
        producer: Pubkey,
        cumulative_work: u128,
    ) -> Self {
        Self {
            height,
            block_hash,
            parent_hash,
            block_time,
            consensus_type: ConsensusType::ProofOfStake,
            difficulty_bits: 0,
            producer,
            cumulative_work,
        }
    }

    pub fn is_better_than(&self, other: &Self) -> bool {
        self.cumulative_work > other.cumulative_work
            || (self.cumulative_work == other.cumulative_work && self.height > other.height)
    }
}

/// Registry of the best-known tip from each peer.
#[derive(Default)]
pub struct PeerTipRegistry {
    tips: std::collections::HashMap<Pubkey, BlockAnnouncement>,
}

impl PeerTipRegistry {
    pub fn new() -> Self { Self::default() }

    pub fn update(&mut self, peer: Pubkey, announcement: BlockAnnouncement) -> bool {
        let is_better = self.tips.get(&peer)
            .map(|existing| announcement.is_better_than(existing))
            .unwrap_or(true);
        if is_better {
            self.tips.insert(peer, announcement);
        }
        is_better
    }

    /// Find the best-known tip across all peers.
    pub fn best_tip(&self) -> Option<&BlockAnnouncement> {
        self.tips.values().max_by_key(|a| a.cumulative_work)
    }

    pub fn peer_count(&self) -> usize { self.tips.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_best_tip() {
        let mut reg = PeerTipRegistry::new();
        let peer_a = Pubkey::new_unique();
        let peer_b = Pubkey::new_unique();
        let ann_a = BlockAnnouncement::new_pow(10, [1u8;32], [0u8;32], 1000, 20, peer_a, 1_000_000);
        let ann_b = BlockAnnouncement::new_pow(11, [2u8;32], [1u8;32], 1060, 20, peer_b, 2_000_000);
        reg.update(peer_a, ann_a);
        reg.update(peer_b, ann_b);
        assert_eq!(reg.best_tip().unwrap().cumulative_work, 2_000_000);
    }
}
