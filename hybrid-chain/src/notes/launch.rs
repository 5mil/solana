//! Launch membership: hidden window, adaptive fill, folded accumulator.
//!
//! 1. Hide the epoch. Spends prove membership in the *sorted* window
//!    (last W sealed epochs + live). Sort order is not time order, so the
//!    path rank does not name an epoch. epoch_index / epoch_root stay off
//!    the wire.
//! 2. Keep epochs populated. Seal pads to a profile bucket with
//!    deterministic dummy leaves so sparse and dense networks produce
//!    the same-shaped epochs. Coinbase already stay-in-tree.
//! 3. Fold. Each seal updates a 32-byte accumulator. Header commitment
//!    is H(window_root || forest_acc). Verify is constant-size. Notes that
//!    fall out of the window must refresh into live — that is how we stay
//!    bounded past any chain length.

use super::tree::{merkle_root, NoteCommitmentTree};
use crate::consensus::pow::sha256d;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const WINDOW_DEPTH: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProfileKind {
    Constrained,
    Sparse,
    Standard,
    Dense,
}

impl ProfileKind {
    pub fn cap(self) -> usize {
        match self {
            Self::Constrained => 64,
            Self::Sparse => 256,
            Self::Standard => 1024,
            Self::Dense => 4096,
        }
    }

    pub fn window_epochs(self) -> usize {
        match self {
            Self::Constrained => 4,
            Self::Sparse => 8,
            Self::Standard => 16,
            Self::Dense => 32,
        }
    }

    pub fn bucket(self) -> usize {
        self.cap()
    }

    pub fn window_leaf_budget(self) -> usize {
        self.window_epochs() * self.bucket() + self.cap()
    }

    pub fn recommend(notes_per_hour: u64, ram_mb: u32) -> Self {
        if ram_mb < 64 || notes_per_hour < 16 {
            return Self::Constrained;
        }
        if ram_mb < 256 || notes_per_hour < 256 {
            return Self::Sparse;
        }
        if ram_mb < 1024 || notes_per_hour < 4096 {
            return Self::Standard;
        }
        Self::Dense
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HiddenProof {
    pub leaf: [u8; 32],
    pub sides: u32,
    pub siblings: [[u8; 32]; WINDOW_DEPTH],
    pub used: u8,
}

impl HiddenProof {
    pub fn size_bytes() -> usize {
        32 + 4 + WINDOW_DEPTH * 32 + 1
    }

    pub fn verify(&self, window_root: [u8; 32]) -> bool {
        if self.used as usize > WINDOW_DEPTH {
            return false;
        }
        let mut hash = self.leaf;
        let mut sides = self.sides;
        for i in 0..self.used as usize {
            let sib = self.siblings[i];
            let mut c = Vec::with_capacity(64);
            if sides & 1 == 0 {
                c.extend_from_slice(&hash);
                c.extend_from_slice(&sib);
            } else {
                c.extend_from_slice(&sib);
                c.extend_from_slice(&hash);
            }
            hash = sha256d(&c);
            sides >>= 1;
        }
        hash == window_root
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaunchSet {
    pub profile: ProfileKind,
    live: Vec<[u8; 32]>,
    window: VecDeque<Vec<[u8; 32]>>,
    forest_acc: [u8; 32],
    sealed_count: u64,
}

impl LaunchSet {
    pub fn new(profile: ProfileKind) -> Self {
        Self {
            profile,
            live: Vec::new(),
            window: VecDeque::new(),
            forest_acc: [0u8; 32],
            sealed_count: 0,
        }
    }

    pub fn standard() -> Self {
        Self::new(ProfileKind::Standard)
    }

    pub fn live_len(&self) -> usize {
        self.live.len()
    }

    pub fn sealed_count(&self) -> u64 {
        self.sealed_count
    }

    pub fn forest_acc(&self) -> [u8; 32] {
        self.forest_acc
    }

    pub fn window_epoch_count(&self) -> usize {
        self.window.len()
    }

    fn window_leaves_unsorted(&self) -> Vec<[u8; 32]> {
        let mut out = Vec::new();
        for epoch in &self.window {
            out.extend_from_slice(epoch);
        }
        out.extend_from_slice(&self.live);
        out
    }

    fn sorted_window(&self) -> Vec<[u8; 32]> {
        let mut leaves = self.window_leaves_unsorted();
        leaves.sort_unstable();
        leaves
    }

    pub fn window_root(&self) -> [u8; 32] {
        merkle_root(&self.sorted_window())
    }

    pub fn commitment(&self) -> [u8; 32] {
        let mut c = Vec::with_capacity(64);
        c.extend_from_slice(&self.window_root());
        c.extend_from_slice(&self.forest_acc);
        sha256d(&c)
    }

    pub fn contains(&self, leaf: [u8; 32]) -> bool {
        self.window_leaves_unsorted().iter().any(|l| *l == leaf)
    }

    pub fn append(&mut self, leaf: [u8; 32]) {
        self.live.push(leaf);
        if self.live.len() >= self.profile.cap() {
            self.seal();
        }
    }

    pub fn seal(&mut self) {
        if self.live.is_empty() {
            return;
        }
        self.pad_live();
        let epoch_root = merkle_root(&self.live);
        let mut acc = Vec::with_capacity(64);
        acc.extend_from_slice(&self.forest_acc);
        acc.extend_from_slice(&epoch_root);
        self.forest_acc = sha256d(&acc);
        self.window.push_back(std::mem::take(&mut self.live));
        self.sealed_count += 1;
        while self.window.len() > self.profile.window_epochs() {
            self.window.pop_front();
        }
    }

    fn pad_live(&mut self) {
        let bucket = self.profile.bucket();
        let mut i = self.live.len() as u64;
        while self.live.len() < bucket {
            let mut buf = Vec::with_capacity(48);
            buf.extend_from_slice(b"epoch-pad");
            buf.extend_from_slice(&self.forest_acc);
            buf.extend_from_slice(&self.sealed_count.to_le_bytes());
            buf.extend_from_slice(&i.to_le_bytes());
            self.live.push(sha256d(&buf));
            i += 1;
        }
    }

    pub fn prove(&self, leaf: [u8; 32]) -> Option<HiddenProof> {
        let sorted = self.sorted_window();
        let index = sorted.iter().position(|l| *l == leaf)?;
        let raw = path_with_sides(&sorted, index)?;
        let mut siblings = [[0u8; 32]; WINDOW_DEPTH];
        let used = raw.1.len().min(WINDOW_DEPTH);
        for (i, s) in raw.1.iter().take(used).enumerate() {
            siblings[i] = *s;
        }
        Some(HiddenProof {
            leaf,
            sides: raw.0,
            siblings,
            used: used as u8,
        })
    }
}

fn path_with_sides(leaves: &[[u8; 32]], mut index: usize) -> Option<(u32, Vec<[u8; 32]>)> {
    if leaves.is_empty() || index >= leaves.len() {
        return None;
    }
    let mut hashes = leaves.to_vec();
    let mut siblings = Vec::new();
    let mut sides = 0u32;
    let mut bit = 0;
    while hashes.len() > 1 {
        if hashes.len() % 2 != 0 {
            hashes.push(*hashes.last().unwrap());
        }
        if index % 2 == 1 {
            sides |= 1 << bit;
            siblings.push(hashes[index - 1]);
        } else {
            siblings.push(hashes[index + 1]);
        }
        hashes = hashes
            .chunks(2)
            .map(|pair| {
                let mut c = pair[0].to_vec();
                c.extend_from_slice(&pair[1]);
                sha256d(&c)
            })
            .collect();
        index /= 2;
        bit += 1;
        if bit >= 32 {
            break;
        }
    }
    Some((sides, siblings))
}

pub fn needs_refresh(set: &LaunchSet, leaf: [u8; 32]) -> bool {
    !set.contains(leaf)
}

#[allow(dead_code)]
fn _tree_compat(leaves: &[[u8; 32]]) -> NoteCommitmentTree {
    let mut t = NoteCommitmentTree::new();
    for l in leaves {
        t.append(*l);
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_proof_has_no_epoch_fields_and_fixed_size() {
        assert_eq!(HiddenProof::size_bytes(), 32 + 4 + 20 * 32 + 1);
        let mut s = LaunchSet::new(ProfileKind::Constrained);
        s.append([7u8; 32]);
        s.append([3u8; 32]);
        let p = s.prove([7u8; 32]).unwrap();
        assert!(p.verify(s.window_root()));
        let encoded = format!("{p:?}");
        assert!(!encoded.contains("epoch_index"));
        assert!(!encoded.contains("epoch_root"));
    }

    #[test]
    fn sort_hides_insertion_order() {
        let mut s = LaunchSet::new(ProfileKind::Constrained);
        s.append([9u8; 32]);
        s.append([1u8; 32]);
        s.append([5u8; 32]);
        let p = s.prove([9u8; 32]).unwrap();
        assert!(p.verify(s.window_root()));
    }

    #[test]
    fn seal_pads_and_folds_constant_acc() {
        let mut s = LaunchSet::new(ProfileKind::Constrained);
        s.append([1u8; 32]);
        s.seal();
        assert_eq!(s.sealed_count(), 1);
        assert_eq!(s.live_len(), 0);
        assert_ne!(s.forest_acc(), [0u8; 32]);
        assert_eq!(s.commitment().len(), 32);
        assert_eq!(
            s.window.back().unwrap().len(),
            ProfileKind::Constrained.bucket()
        );
    }

    #[test]
    fn dropped_epoch_requires_refresh() {
        let mut s = LaunchSet::new(ProfileKind::Constrained);
        let first = [42u8; 32];
        s.append(first);
        s.seal();
        for e in 0..ProfileKind::Constrained.window_epochs() {
            s.append([(e + 2) as u8; 32]);
            s.seal();
        }
        assert!(needs_refresh(&s, first));
        assert!(!s.contains(first));
    }

    #[test]
    fn recommend_adapts_to_conditions() {
        assert_eq!(ProfileKind::recommend(1, 16), ProfileKind::Constrained);
        assert_eq!(ProfileKind::recommend(100, 128), ProfileKind::Sparse);
        assert_eq!(ProfileKind::recommend(1000, 512), ProfileKind::Standard);
        assert_eq!(ProfileKind::recommend(10_000, 4096), ProfileKind::Dense);
    }

    #[test]
    fn proof_size_stable_across_profiles() {
        for p in [
            ProfileKind::Constrained,
            ProfileKind::Sparse,
            ProfileKind::Standard,
        ] {
            let mut s = LaunchSet::new(p);
            s.append([2u8; 32]);
            let proof = s.prove([2u8; 32]).unwrap();
            assert_eq!(proof.siblings.len(), WINDOW_DEPTH);
            assert!(proof.verify(s.window_root()));
        }
    }
}
