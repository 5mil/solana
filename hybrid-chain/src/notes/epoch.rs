//! Bounded-epoch note forest.
//!
//! Full-chain membership trees make prover work and node RAM grow with |notes|.
//! This forest seals a live tree every `cap` leaves. A spend proves:
//!   1. leaf is in an epoch tree of fixed depth
//!   2. that epoch root is in the forest commitment
//!
//! Proof bytes are padded to EPOCH_DEPTH / FOREST_DEPTH, so size does not
//! grow with the chain. Prover work is O(cap) not O(total notes).

use super::tree::{merkle_root, NoteCommitmentTree};
use crate::consensus::pow::sha256d;
use serde::{Deserialize, Serialize};

pub const EPOCH_CAP: usize = 1 << 16;
pub const EPOCH_DEPTH: usize = 16;
pub const FOREST_DEPTH: usize = 16;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochForest {
    pub cap: usize,
    sealed: Vec<[u8; 32]>,
    live: NoteCommitmentTree,
}

impl Default for EpochForest {
    fn default() -> Self {
        Self::with_cap(EPOCH_CAP)
    }
}

impl EpochForest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_cap(cap: usize) -> Self {
        assert!(cap >= 2);
        Self {
            cap,
            sealed: Vec::new(),
            live: NoteCommitmentTree::new(),
        }
    }

    pub fn live(&self) -> &NoteCommitmentTree {
        &self.live
    }

    pub fn sealed(&self) -> &[[u8; 32]] {
        &self.sealed
    }

    pub fn len(&self) -> usize {
        self.sealed.len() * self.cap + self.live.len()
    }

    pub fn append(&mut self, leaf: [u8; 32]) {
        self.live.append(leaf);
        if self.live.len() >= self.cap {
            self.sealed.push(self.live.root());
            self.live = NoteCommitmentTree::new();
        }
    }

    pub fn commitment(&self) -> [u8; 32] {
        if self.sealed.is_empty() {
            return self.live.root();
        }
        let mut items = self.sealed.clone();
        if !self.live.is_empty() {
            items.push(self.live.root());
        }
        merkle_root(&items)
    }

    pub fn root(&self) -> [u8; 32] {
        self.commitment()
    }

    fn epoch_items(&self) -> Vec<[u8; 32]> {
        let mut items = self.sealed.clone();
        if !self.live.is_empty() {
            items.push(self.live.root());
        }
        items
    }

    pub fn prove_live(&self, index_in_live: usize) -> Option<ScaleProof> {
        if index_in_live >= self.live.len() {
            return None;
        }
        let leaf = self.live.leaves()[index_in_live];
        let raw = self.live.proof(index_in_live)?;
        let epoch_root = self.live.root();
        let epoch_index = self.sealed.len() as u32;
        Some(self.finish_proof(leaf, index_in_live as u32, raw, epoch_root, epoch_index))
    }

    pub fn prove_sealed(
        &self,
        leaf: [u8; 32],
        index_in_epoch: u32,
        in_epoch_path: Vec<[u8; 32]>,
        epoch_index: u32,
    ) -> Option<ScaleProof> {
        let epoch_root = *self.sealed.get(epoch_index as usize)?;
        if !NoteCommitmentTree::verify_inclusion(
            leaf,
            index_in_epoch as usize,
            &in_epoch_path,
            epoch_root,
        ) {
            return None;
        }
        Some(self.finish_proof(leaf, index_in_epoch, in_epoch_path, epoch_root, epoch_index))
    }

    fn finish_proof(
        &self,
        leaf: [u8; 32],
        index_in_epoch: u32,
        raw_path: Vec<[u8; 32]>,
        epoch_root: [u8; 32],
        epoch_index: u32,
    ) -> ScaleProof {
        let mut path = [[0u8; 32]; EPOCH_DEPTH];
        let used = raw_path.len().min(EPOCH_DEPTH);
        for (i, p) in raw_path.iter().take(used).enumerate() {
            path[i] = *p;
        }
        let items = self.epoch_items();
        let epoch_raw = forest_path(&items, epoch_index as usize);
        let mut epoch_path = [[0u8; 32]; FOREST_DEPTH];
        let epoch_used = epoch_raw.len().min(FOREST_DEPTH);
        for (i, p) in epoch_raw.iter().take(epoch_used).enumerate() {
            epoch_path[i] = *p;
        }
        ScaleProof {
            leaf,
            index_in_epoch,
            path,
            path_used: used as u8,
            epoch_root,
            epoch_index,
            epoch_path,
            epoch_path_used: epoch_used as u8,
        }
    }
}

fn forest_path(items: &[[u8; 32]], mut index: usize) -> Vec<[u8; 32]> {
    if items.is_empty() || index >= items.len() {
        return Vec::new();
    }
    let mut hashes = items.to_vec();
    let mut path = Vec::new();
    while hashes.len() > 1 {
        if hashes.len() % 2 != 0 {
            hashes.push(*hashes.last().unwrap());
        }
        let sibling = if index % 2 == 0 {
            hashes[index + 1]
        } else {
            hashes[index - 1]
        };
        path.push(sibling);
        hashes = hashes
            .chunks(2)
            .map(|pair| {
                let mut c = pair[0].to_vec();
                c.extend_from_slice(&pair[1]);
                sha256d(&c)
            })
            .collect();
        index /= 2;
    }
    path
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScaleProof {
    pub leaf: [u8; 32],
    pub index_in_epoch: u32,
    pub path: [[u8; 32]; EPOCH_DEPTH],
    pub path_used: u8,
    pub epoch_root: [u8; 32],
    pub epoch_index: u32,
    pub epoch_path: [[u8; 32]; FOREST_DEPTH],
    pub epoch_path_used: u8,
}

impl ScaleProof {
    pub fn size_bytes() -> usize {
        32 + 4 + EPOCH_DEPTH * 32 + 1 + 32 + 4 + FOREST_DEPTH * 32 + 1
    }

    pub fn verify(&self, forest_commitment: [u8; 32]) -> bool {
        if self.path_used as usize > EPOCH_DEPTH || self.epoch_path_used as usize > FOREST_DEPTH {
            return false;
        }
        let in_path = &self.path[..self.path_used as usize];
        if !NoteCommitmentTree::verify_inclusion(
            self.leaf,
            self.index_in_epoch as usize,
            in_path,
            self.epoch_root,
        ) {
            return false;
        }
        if self.epoch_path_used == 0 {
            return forest_commitment == self.epoch_root;
        }
        let epath = &self.epoch_path[..self.epoch_path_used as usize];
        NoteCommitmentTree::verify_inclusion(
            self.epoch_root,
            self.epoch_index as usize,
            epath,
            forest_commitment,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_size_is_constant() {
        assert_eq!(ScaleProof::size_bytes(), 32 + 4 + 512 + 1 + 32 + 4 + 512 + 1);
    }

    #[test]
    fn live_proof_before_first_seal_matches_tree_root() {
        let mut f = EpochForest::with_cap(8);
        f.append([1u8; 32]);
        f.append([2u8; 32]);
        assert_eq!(f.commitment(), f.live().root());
        let p = f.prove_live(1).unwrap();
        assert!(p.verify(f.commitment()));
    }

    #[test]
    fn seal_keeps_fixed_layout() {
        let mut f = EpochForest::with_cap(2);
        f.append([1u8; 32]);
        f.append([2u8; 32]);
        f.append([3u8; 32]);
        assert_eq!(f.sealed().len(), 1);
        let live = f.prove_live(0).unwrap();
        assert!(live.verify(f.commitment()));
        assert_eq!(live.path.len(), EPOCH_DEPTH);
        assert_eq!(live.epoch_path.len(), FOREST_DEPTH);
    }
}
