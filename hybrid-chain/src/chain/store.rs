use crate::chain::block::{Block, BlockType};
use crate::chain::blockchain::Blockchain;
use crate::consensus::pow::{meets_difficulty, sha256d};
use crate::notes::action::emission_commitment;
use crate::notes::launch::LaunchSet;
use crate::notes::tags::SpendTagSet;
use crate::notes::tree::NoteCommitmentTree;
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
        let bytes =
            bincode::serialize(&self.snapshot()).map_err(|e| StoreError::Io(e.to_string()))?;
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
        let snap: ChainSnapshot =
            bincode::deserialize(&bytes).map_err(|e| StoreError::Decode(e.to_string()))?;
        if snap.blocks.is_empty() {
            return Err(StoreError::Invalid("snapshot contains no blocks".into()));
        }
        let mut chain = Blockchain {
            blocks: Vec::new(),
            block_index: Default::default(),
            current_difficulty: snap.current_difficulty,
            total_supply: snap.total_supply,
            notes: NoteCommitmentTree::new(),
            launch: LaunchSet::standard(),
            tags: SpendTagSet::new(),
        };
        for (i, block) in snap.blocks.into_iter().enumerate() {
            let hash = block.hash();
            chain.block_index.insert(hash, i);
            chain.blocks.push(block);
        }
        chain.revalidate()?;
        chain
            .rebuild_notes()
            .map_err(|e| StoreError::Invalid(e.to_string()))?;
        Ok(chain)
    }

    pub fn revalidate(&self) -> Result<(), StoreError> {
        if self.blocks.is_empty() {
            return Err(StoreError::Invalid("empty chain".into()));
        }
        let mut notes = NoteCommitmentTree::new();
        let mut launch = LaunchSet::standard();
        let mut tags = SpendTagSet::new();
        for (i, block) in self.blocks.iter().enumerate() {
            if block.header.height != i as u64 {
                return Err(StoreError::Invalid(format!(
                    "height mismatch at index {i}: header={}",
                    block.header.height
                )));
            }
            let expected_merkle = Block::compute_merkle_root(&block.transactions);
            if expected_merkle != block.header.merkle_root {
                return Err(StoreError::Invalid(format!(
                    "transaction commitment mismatch at height {i}"
                )));
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
            if block.header.block_type == BlockType::PoW {
                let header_bytes = bincode::serialize(&block.header)
                    .map_err(|e| StoreError::Decode(e.to_string()))?;
                let hash = sha256d(&header_bytes);
                if hash != block.hash() {
                    return Err(StoreError::Invalid(format!("hash mismatch at height {i}")));
                }
                if i > 0 && !meets_difficulty(&hash, block.header.difficulty) {
                    return Err(StoreError::Invalid(format!(
                        "insufficient work at height {i}"
                    )));
                }
                let expected_reward = pow_reward_at_height(block.header.height);
                let expected_c = emission_commitment(block.header.height, expected_reward).commitment;
                let found = block.compact.iter().any(|b| {
                    b.is_emission()
                        && b.real_outputs()
                            .iter()
                            .any(|o| o.value_commitment.commitment == expected_c)
                });
                if !found {
                    return Err(StoreError::Invalid(format!(
                        "emission commitment mismatch at height {i}"
                    )));
                }
                for tx in &block.transactions {
                    for o in &tx.outputs {
                        if o.value != 0 {
                            return Err(StoreError::Invalid(format!(
                                "plaintext value forbidden at height {i}"
                            )));
                        }
                    }
                }
            }
            for bundle in &block.compact {
                if !bundle.verify_conservation() {
                    return Err(StoreError::Invalid(format!(
                        "compact conservation failed at height {i}"
                    )));
                }
                crate::chain::blockchain::verify_bundle_against(
                    &notes,
                    &launch,
                    bundle,
                    bundle.is_emission(),
                )
                .map_err(|e| StoreError::Invalid(format!("compact verify at height {i}: {e}")))?;
                for tag in bundle.spend_tags() {
                    tags.insert(tag).map_err(|e| {
                        StoreError::Invalid(format!("spend tag at height {i}: {e}"))
                    })?;
                }
                for c in bundle.output_commitments() {
                    notes.append(c);
                    launch.append(c);
                }
            }
            if block.header.notes_root != notes.root() {
                return Err(StoreError::Invalid(format!(
                    "notes_root mismatch at height {i}"
                )));
            }
            if block.header.tags_root != tags.root() {
                return Err(StoreError::Invalid(format!(
                    "tags_root mismatch at height {i}"
                )));
            }
        }
        Ok(())
    }
}

fn pow_reward_at_height(height: u64) -> u64 {
    let halvings = height / 210_000;
    if halvings >= 64 {
        0
    } else {
        CHAIN_PARAMS.pow_block_reward >> halvings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::action::emission_blinding;
    use std::env;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

    fn isolated_path(label: &str) -> std::path::PathBuf {
        let n = TEST_SEQ.fetch_add(1, Ordering::Relaxed);
        env::temp_dir().join(format!(
            "hybrid-chain-{}-{}-{}",
            label,
            std::process::id(),
            n
        ))
    }

    fn mined_chain() -> (Blockchain, std::path::PathBuf) {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("disk-miner");
        let dir = isolated_path("ok");
        let path = dir.join("chain.bin");
        chain.save_to_path(&path).expect("save");
        (chain, path)
    }

    fn rewrite(path: &Path, mutate: impl FnOnce(&mut ChainSnapshot)) {
        let bytes = fs::read(path).unwrap();
        let mut snap: ChainSnapshot = bincode::deserialize(&bytes).unwrap();
        mutate(&mut snap);
        fs::write(path, bincode::serialize(&snap).unwrap()).unwrap();
    }

    #[test]
    fn persist_restart_preserves_height_and_tip() {
        let (chain, path) = mined_chain();
        let loaded = Blockchain::load_from_path(&path).expect("trusted load");
        assert_eq!(loaded.height(), chain.height());
        assert_eq!(loaded.tip_hash(), chain.tip_hash());
        assert_eq!(loaded.height(), 2);
        assert_eq!(loaded.launch.commitment(), chain.launch.commitment());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn persist_after_hidden_spend_rebuilds_window() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("miner");
        let height = 1u64;
        let sealed = crate::notes::payout::SealedPayout::from_ticket(b"miner", height);
        let reward = CHAIN_PARAMS.pow_block_reward;
        let in_blind = emission_blinding(height);
        let r_out = crate::notes::commitment::blinding_from_seed(b"wout");
        let r_fee = in_blind - r_out;
        let bundle = crate::notes::transfer_window_bundle(
            &sealed.spend_secret(),
            &chain.launch,
            reward,
            &in_blind,
            [7u8; 32],
            b"recv-scan",
            reward - 1,
            &r_out,
            1,
            &r_fee,
            [0u8; 32],
        )
        .expect("window transfer");
        let _ = chain.mine_pow_with_bundles("miner2", vec![bundle]);
        let dir = isolated_path("hidden");
        let path = dir.join("chain.bin");
        chain.save_to_path(&path).expect("save");
        let loaded = Blockchain::load_from_path(&path).expect("load hidden spend chain");
        assert_eq!(loaded.height(), chain.height());
        assert_eq!(loaded.launch.commitment(), chain.launch.commitment());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_tampered_nonce() {
        let (_chain, path) = mined_chain();
        rewrite(&path, |snap| {
            snap.blocks[1].header.nonce = snap.blocks[1].header.nonce.wrapping_add(1);
        });
        assert!(Blockchain::load_from_path(&path).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_tampered_parent_hash() {
        let (_chain, path) = mined_chain();
        rewrite(&path, |snap| {
            snap.blocks[1].header.prev_hash[0] ^= 0xFF;
        });
        assert!(Blockchain::load_from_path(&path).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_tampered_transaction_commitment() {
        let (_chain, path) = mined_chain();
        rewrite(&path, |snap| {
            snap.blocks[1].header.merkle_root[0] ^= 0xAA;
        });
        assert!(Blockchain::load_from_path(&path).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_plaintext_value() {
        let (_chain, path) = mined_chain();
        rewrite(&path, |snap| {
            snap.blocks[1].transactions[0].outputs[0].value = 99;
            snap.blocks[1].header.merkle_root =
                Block::compute_merkle_root(&snap.blocks[1].transactions);
        });
        assert!(Blockchain::load_from_path(&path).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_tampered_notes_root() {
        let (_chain, path) = mined_chain();
        rewrite(&path, |snap| {
            snap.blocks[1].header.notes_root[0] ^= 0x5A;
        });
        assert!(Blockchain::load_from_path(&path).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_tampered_compact_commitment() {
        let (_chain, path) = mined_chain();
        rewrite(&path, |snap| {
            if let Some(out) = snap.blocks[1].compact[0].actions[0].output.as_mut() {
                out.value_commitment.commitment[0] ^= 0x11;
            }
        });
        assert!(Blockchain::load_from_path(&path).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_truncated_payload() {
        let (_chain, path) = mined_chain();
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.len() > 8);
        fs::write(&path, &bytes[..bytes.len() / 3]).unwrap();
        assert!(Blockchain::load_from_path(&path).is_err());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn rejects_missing_file() {
        let path = isolated_path("missing").join("chain.bin");
        match Blockchain::load_from_path(&path) {
            Err(StoreError::Io(msg)) => assert!(msg.contains("missing"), "{msg}"),
            other => panic!("expected missing-file Io error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_empty_file() {
        let dir = isolated_path("empty");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chain.bin");
        fs::write(&path, b"").unwrap();
        match Blockchain::load_from_path(&path) {
            Err(StoreError::Decode(_)) => {}
            other => panic!("expected Decode for empty file, got {other:?}"),
        }
        let _ = fs::remove_dir_all(dir);
    }
}
