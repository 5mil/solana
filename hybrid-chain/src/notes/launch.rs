//! Living set = current window of commitments. No listed ring API.

use super::tree::{merkle_root, NoteCommitmentTree};
use crate::consensus::pow::sha256d;
use curve25519_dalek::ristretto::RistrettoPoint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::collections::VecDeque;

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
    pub fn bucket(self) -> usize { self.cap() }
    pub fn id(self) -> u8 {
        match self {
            Self::Constrained => 1,
            Self::Sparse => 2,
            Self::Standard => 3,
            Self::Dense => 4,
        }
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
        Self { profile, live: Vec::new(), window: VecDeque::new(), forest_acc: [0u8; 32], sealed_count: 0 }
    }
    pub fn standard() -> Self { Self::new(ProfileKind::Standard) }
    pub fn live_leaves(&self) -> Vec<[u8; 32]> {
        let mut out = Vec::new();
        for epoch in &self.window { out.extend_from_slice(epoch); }
        out.extend_from_slice(&self.live);
        out
    }
    fn sorted(&self) -> Vec<[u8; 32]> {
        let mut l = self.live_leaves();
        l.sort_unstable();
        l
    }
    pub fn window_root(&self) -> [u8; 32] { merkle_root(&self.sorted()) }
    pub fn commitment(&self) -> [u8; 32] {
        let mut c = Vec::with_capacity(65);
        c.extend_from_slice(&self.window_root());
        c.extend_from_slice(&self.forest_acc);
        c.push(self.profile.id());
        sha256d(&c)
    }
    pub fn contains(&self, leaf: [u8; 32]) -> bool {
        self.live_leaves().iter().any(|l| *l == leaf)
    }
    pub fn append(&mut self, leaf: [u8; 32]) {
        self.live.push(leaf);
        if self.live.len() >= self.profile.cap() { self.seal(); }
    }
    pub fn seal(&mut self) {
        if self.live.is_empty() { return; }
        self.pad_live();
        let epoch_root = merkle_root(&self.live);
        let mut acc = Vec::with_capacity(64);
        acc.extend_from_slice(&self.forest_acc);
        acc.extend_from_slice(&epoch_root);
        self.forest_acc = sha256d(&acc);
        self.window.push_back(std::mem::take(&mut self.live));
        self.sealed_count += 1;
        while self.window.len() > self.profile.window_epochs() { self.window.pop_front(); }
    }
    fn pad_live(&mut self) {
        let bucket = self.profile.bucket();
        let mut i = self.live.len() as u64;
        while self.live.len() < bucket {
            self.live.push(pad_point(&self.forest_acc, self.sealed_count, i));
            i += 1;
        }
    }
    pub fn prove_window(&self, leaf: [u8; 32]) -> Option<(usize, Vec<[u8; 32]>)> {
        let sorted = self.sorted();
        let index = sorted.iter().position(|l| *l == leaf)?;
        let mut t = NoteCommitmentTree::new();
        for l in &sorted { t.append(*l); }
        Some((index, t.proof(index)?))
    }
}

fn pad_point(acc: &[u8; 32], sealed: u64, i: u64) -> [u8; 32] {
    let mut h = Sha512::new();
    h.update(b"epoch-pad-point");
    h.update(acc);
    h.update(&sealed.to_le_bytes());
    h.update(&i.to_le_bytes());
    let out = h.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&out);
    RistrettoPoint::from_uniform_bytes(&wide).compress().to_bytes()
}
