use crate::chain::block::{Block, BlockType};
use crate::chain::blockchain::Blockchain;
use crate::consensus::pow::{meets_difficulty, sha256d};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize)]
pub struct ChainSnapshot {
    pub blocks: Vec<Block>,
    pub current_difficulty: u32,
    pub total_supply: u64,
}

#[derive(Debug)]
pub enum StoreError {
    Io(String),
    Decode(String),
    Invalid(String),
}

impl Blockchain {
    pub fn snapshot(&self) -> ChainSnapshot {
        ChainSnapshot {
            blocks: self.blocks.clone(),
            current_difficulty: self.current_difficulty,
            total_supply: self.total_supply,
        }
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), StoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| StoreError::Io(e.to_string()))?;
        }
        let bytes = bincode::serialize(&self.snapshot()).map_err(|e| StoreError::Io(e.to_string()))?;
        fs::write(path, bytes).map_err(|e| StoreError::Io(e.to_string()))
    }

    pub fn load_from_path(path: &Path) -> Result<Self, StoreError> {
        let bytes = fs::read(path).map_err(|e| StoreError::Io(e.to_string()))?;
        let snap: ChainSnapshot =
            bincode::deserialize(&bytes).map_err(|e| StoreError::Decode(e.to_string()))?;
        let mut chain = Blockchain {
            blocks: Vec::new(),
            block_index: Default::default(),
            current_difficulty: snap.current_difficulty,
            total_supply: snap.total_supply,
        };
        for (i, block) in snap.blocks.into_iter().enumerate() {
            let hash = block.hash();
            chain.block_index.insert(hash, i);
            chain.blocks.push(block);
        }
        Ok(chain)
    }

    pub fn revalidate(&self) -> Result<(), StoreError> {
        if self.blocks.is_empty() {
            return Err(StoreError::Invalid("empty chain".into()));
        }
        for (i, block) in self.blocks.iter().enumerate() {
            if block.header.height != i as u64 {
                return Err(StoreError::Invalid(format!(
                    "height mismatch at index {i}: header={}",
                    block.header.height
                )));
            }
            let expected_merkle = Block::compute_merkle_root(&block.transactions);
            if expected_merkle != block.header.merkle_root {
                return Err(StoreError::Invalid(format!("bad merkle at height {i}")));
            }
            if i == 0 {
                if block.header.prev_hash != [0u8; 32] {
                    return Err(StoreError::Invalid("genesis prev_hash must be zero".into()));
                }
            } else {
                let parent_hash = self.blocks[i - 1].hash();
                if block.header.prev_hash != parent_hash {
                    return Err(StoreError::Invalid(format!("bad parent at height {i}")));
                }
            }
            if block.header.block_type == BlockType::PoW && i > 0 {
                let header_bytes = bincode::serialize(&block.header)
                    .map_err(|e| StoreError::Decode(e.to_string()))?;
                let hash = sha256d(&header_bytes);
                if hash != block.hash() {
                    return Err(StoreError::Invalid(format!("hash mismatch at height {i}")));
                }
                if !meets_difficulty(&hash, block.header.difficulty) {
                    return Err(StoreError::Invalid(format!(
                        "insufficient work at height {i}"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn persist_and_revalidate_pow_block() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("disk-miner");
        let dir = env::temp_dir().join(format!("hybrid-chain-test-{}", std::process::id()));
        let path = dir.join("chain.bin");
        chain.save_to_path(&path).expect("save");
        let loaded = Blockchain::load_from_path(&path).expect("load");
        loaded.revalidate().expect("revalidate");
        assert_eq!(loaded.height(), chain.height());
        assert_eq!(loaded.tip_hash(), chain.tip_hash());
        let _ = fs::remove_dir_all(dir);
    }
}
