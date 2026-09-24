use crate::chain::block::{Block, BlockType};
use crate::chain::blockchain::Blockchain;
use crate::consensus::pow::{meets_difficulty, sha256d};
use crate::notes::launch::LaunchSet;
use crate::notes::tags::SpendTagSet;
use crate::params::CHAIN_PARAMS;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Serialize, Deserialize, Clone)]
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
        if !path.exists() {
            return Err(StoreError::Io(format!("missing chain file: {}", path.display())));
        }
        let bytes = fs::read(path).map_err(|e| StoreError::Io(e.to_string()))?;
        if bytes.is_empty() {
            return Err(StoreError::Decode("empty chain file".into()));
        }
        let snap: ChainSnapshot = bincode::deserialize(&bytes).map_err(|e| StoreError::Decode(e.to_string()))?;
        if snap.blocks.is_empty() {
            return Err(StoreError::Invalid("empty snapshot".into()));
        }
        let mut chain = Blockchain {
            blocks: Vec::new(),
            block_index: Default::default(),
            current_difficulty: snap.current_difficulty,
            total_supply: snap.total_supply,
            launch: LaunchSet::standard(),
            tags: SpendTagSet::new(),
        };
        for (i, block) in snap.blocks.into_iter().enumerate() {
            chain.block_index.insert(block.hash(), i);
            chain.blocks.push(block);
        }
        chain.revalidate()?;
        chain.rebuild_notes().map_err(|e| StoreError::Invalid(e.to_string()))?;
        Ok(chain)
    }

    pub fn revalidate(&self) -> Result<(), StoreError> {
        if self.blocks.is_empty() {
            return Err(StoreError::Invalid("empty chain".into()));
        }
        let mut launch = LaunchSet::standard();
        let mut tags = SpendTagSet::new();
        for (i, block) in self.blocks.iter().enumerate() {
            if block.header.height != i as u64 {
                return Err(StoreError::Invalid(format!("height mismatch at {i}")));
            }
            if Block::compute_merkle_root(&block.compact) != block.header.merkle_root {
                return Err(StoreError::Invalid(format!("compact merkle mismatch at {i}")));
            }
            if i == 0 {
                if block.header.prev_hash != [0u8; 32] {
                    return Err(StoreError::Invalid("bad genesis prev".into()));
                }
            } else if block.header.prev_hash != self.blocks[i - 1].hash() {
                return Err(StoreError::Invalid(format!("bad parent at {i}")));
            }
            if block.header.block_type == BlockType::PoW {
                let bytes = bincode::serialize(&block.header).map_err(|e| StoreError::Decode(e.to_string()))?;
                let hash = sha256d(&bytes);
                if hash != block.hash() {
                    return Err(StoreError::Invalid(format!("hash mismatch at {i}")));
                }
                if i > 0 && !meets_difficulty(&hash, block.header.difficulty) {
                    return Err(StoreError::Invalid(format!("weak work at {i}")));
                }
            }
            let reward = if block.header.block_type == BlockType::PoW {
                Some(pow_reward_at_height(block.header.height))
            } else {
                block.compact.iter().find(|b| b.is_emission()).and_then(|b| b.emission.as_ref().map(|e| e.reward))
            };
            for bundle in &block.compact {
                crate::chain::blockchain::verify_bundle_against(
                    &launch, bundle, bundle.is_emission(),
                    if bundle.is_emission() { reward } else { None },
                    block.header.height,
                ).map_err(|e| StoreError::Invalid(format!("bundle at {i}: {e}")))?;
                for tag in bundle.spend_tags() {
                    tags.insert(tag).map_err(|e| StoreError::Invalid(format!("tag at {i}: {e}")))?;
                }
                for n in bundle.output_records() {
                    launch.append_note(n);
                }
            }
            if block.header.notes_root != launch.commitment() {
                return Err(StoreError::Invalid(format!("live root mismatch at {i}")));
            }
        }
        Ok(())
    }
}

fn pow_reward_at_height(height: u64) -> u64 {
    let halvings = height / 210_000;
    if halvings >= 64 { 0 } else { CHAIN_PARAMS.pow_block_reward >> halvings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};
    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);
    fn isolated_path(label: &str) -> std::path::PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!("hc-{}-{}-{}", label, std::process::id(), n))
    }
    #[test]
    fn persist_live_root() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("pool-ticket");
        let dir = isolated_path("ok");
        let path = dir.join("chain.bin");
        chain.save_to_path(&path).unwrap();
        let loaded = Blockchain::load_from_path(&path).expect("load");
        assert_eq!(loaded.launch.commitment(), chain.launch.commitment());
        assert_eq!(loaded.height(), chain.height());
        let _ = fs::remove_dir_all(dir);
    }
}
