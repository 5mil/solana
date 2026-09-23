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
    pub compact: Vec<ActionBundle>,
}

impl Block {
    pub fn hash(&self) -> [u8; 32] {
        sha256d(&bincode::serialize(&self.header).unwrap_or_default())
    }

    pub fn compute_merkle_root(bundles: &[ActionBundle]) -> [u8; 32] {
        if bundles.is_empty() {
            return [0u8; 32];
        }
        let mut hashes: Vec<[u8; 32]> = bundles.iter().map(|b| b.id()).collect();
        while hashes.len() > 1 {
            if hashes.len() % 2 != 0 {
                hashes.push(*hashes.last().unwrap());
            }
            hashes = hashes
                .chunks(2)
                .map(|pair| {
                    let mut c = pair[0].to_vec();
                    c.extend_from_slice(&pair[1]);
                    sha256d(&c)
                })
                .collect();
        }
        hashes[0]
    }
}
