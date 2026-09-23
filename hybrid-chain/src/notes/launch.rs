//! Living set: sorted window for decoys, full history stays spendable.
//! Pads are uniform Ristretto points with no known opening.

use super::auth::RING;
use super::tree::merkle_root;
use crate::consensus::pow::sha256d;
use curve25519_dalek::ristretto::RistrettoPoint;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
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
    history: Vec<[u8; 32]>,
    forest_acc: [u8; 32],
    sealed_count: u64,
}

impl LaunchSet {
    pub fn new(profile: ProfileKind) -> Self {
        Self {
            profile,
            live: Vec::new(),
            window: VecDeque::new(),
            history: Vec::new(),
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

    fn decoys(&self) -> Vec<[u8; 32]> {
        let mut out = Vec::new();
        for epoch in &self.window {
            out.extend_from_slice(epoch);
        }
        out.extend_from_slice(&self.live);
        out
    }

    fn sorted_window(&self) -> Vec<[u8; 32]> {
        let mut leaves = self.decoys();
        leaves.sort_unstable();
        leaves
    }

    pub fn window_root(&self) -> [u8; 32] {
        merkle_root(&self.sorted_window())
    }

    pub fn commitment(&self) -> [u8; 32] {
        let mut c = Vec::with_capacity(65);
        c.extend_from_slice(&self.window_root());
        c.extend_from_slice(&self.forest_acc);
        c.push(self.profile.id());
        sha256d(&c)
    }

    pub fn contains(&self, leaf: [u8; 32]) -> bool {
        self.history.iter().any(|l| *l == leaf) || self.live.iter().any(|l| *l == leaf)
    }

    pub fn append(&mut self, leaf: [u8; 32]) {
        self.live.push(leaf);
        self.history.push(leaf);
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
            let p = pad_point(&self.forest_acc, self.sealed_count, i);
            self.live.push(p);
            self.history.push(p);
            i += 1;
        }
    }

    /// Deterministic ring: real cm plus decoys from the living window.
    /// Archive notes stay spendable (history) without a refresh holiday.
    pub fn sample_ring(&self, real: [u8; 32], seed: &[u8]) -> Option<(Vec<[u8; 32]>, usize)> {
        if !self.contains(real) {
            return None;
        }
        let mut decoys: Vec<[u8; 32]> = self
            .decoys()
            .into_iter()
            .filter(|c| *c != real)
            .collect();
        decoys.sort_unstable();
        let mut ring = Vec::with_capacity(RING);
        ring.push(real);
        let mut i = 0u64;
        while ring.len() < RING && !decoys.is_empty() {
            let mut buf = seed.to_vec();
            buf.extend_from_slice(&i.to_le_bytes());
            let h = sha256d(&buf);
            let idx = u32::from_le_bytes(h[0..4].try_into().unwrap()) as usize % decoys.len();
            let pick = decoys.remove(idx);
            if !ring.contains(&pick) {
                ring.push(pick);
            }
            i += 1;
            if i > 1024 {
                break;
            }
        }
        while ring.len() < RING {
            ring.push(real);
        }
        ring.sort_unstable();
        let index = ring.iter().position(|c| *c == real)?;
        Some((ring, index))
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

pub fn needs_refresh(_set: &LaunchSet, _leaf: [u8; 32]) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pads_are_curve_points() {
        let p = pad_point(&[7u8; 32], 0, 1);
        assert!(curve25519_dalek::ristretto::CompressedRistretto(p)
            .decompress()
            .is_some());
    }

    #[test]
    fn history_survives_window_roll() {
        let mut s = LaunchSet::new(ProfileKind::Constrained);
        let first = [42u8; 32];
        s.append(first);
        s.seal();
        for e in 0..ProfileKind::Constrained.window_epochs() + 2 {
            s.append([(e + 3) as u8; 32]);
            s.seal();
        }
        assert!(s.contains(first));
        assert!(!needs_refresh(&s, first));
        let (ring, idx) = s.sample_ring(first, b"seed").unwrap();
        assert_eq!(ring.len(), RING);
        assert_eq!(ring[idx], first);
    }

    #[test]
    fn commitment_binds_profile() {
        let a = LaunchSet::new(ProfileKind::Standard);
        let b = LaunchSet::new(ProfileKind::Dense);
        assert_ne!(a.commitment(), b.commitment());
    }
}
