use serde::{Deserialize, Serialize};
use crate::consensus::pow::sha256d;
use crate::notes::action::ActionBundle;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BlockType {
    PoW,
    PoS,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockHeader {
    pub version: u32,
    pub height: u64,
    pub prev_hash: [u8; 32],
    pub merkle_root: [u8; 32],
    pub timestamp: i64,
    pub difficulty: u32,
    pub block_type: BlockType,
    pub nonce: u64,
    pub stake_modifier: [u8; 32],
    pub notes_root: [u8; 32],
    pub tags_root: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<crate::chain::transaction::Transaction>,
    pub compact: Vec<ActionBundle>,
}

impl Block {
    pub fn hash(&self) -> [u8; 32] {
        let header_bytes = bincode::serialize(&self.header).unwrap_or_default();
        sha256d(&header_bytes)
    }

    pub fn compute_merkle_root(txs: &[crate::chain::transaction::Transaction]) -> [u8; 32] {
        if txs.is_empty() {
            return [0u8; 32];
        }
        let mut hashes: Vec<[u8; 32]> = txs.iter().map(|tx| tx.txid()).collect();
        while hashes.len() > 1 {
            if hashes.len() % 2 != 0 {
                hashes.push(*hashes.last().unwrap());
            }
            hashes = hashes.chunks(2)
                .map(|pair| {
                    let mut combined = pair[0].to_vec();
                    combined.extend_from_slice(&pair[1]);
                    sha256d(&combined)
                })
                .collect();
        }
        hashes[0]
    }
}
