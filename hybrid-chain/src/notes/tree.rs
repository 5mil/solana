//! Append-only binary Merkle tree of note commitments.

use crate::consensus::pow::sha256d;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteCommitmentTree {
    leaves: Vec<[u8; 32]>,
}

impl NoteCommitmentTree {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    pub fn leaves(&self) -> &[[u8; 32]] {
        &self.leaves
    }

    pub fn append(&mut self, commitment: [u8; 32]) {
        self.leaves.push(commitment);
    }

    pub fn root(&self) -> [u8; 32] {
        merkle_root(&self.leaves)
    }

    pub fn proof(&self, index: usize) -> Option<Vec<[u8; 32]>> {
        if index >= self.leaves.len() {
            return None;
        }
        let mut hashes = self.leaves.clone();
        let mut path = Vec::new();
        let mut i = index;
        while hashes.len() > 1 {
            if hashes.len() % 2 != 0 {
                hashes.push(*hashes.last().unwrap());
            }
            let sibling = if i % 2 == 0 { hashes[i + 1] } else { hashes[i - 1] };
            path.push(sibling);
            hashes = hashes
                .chunks(2)
                .map(|pair| {
                    let mut c = pair[0].to_vec();
                    c.extend_from_slice(&pair[1]);
                    sha256d(&c)
                })
                .collect();
            i /= 2;
        }
        Some(path)
    }

    pub fn verify_inclusion(leaf: [u8; 32], index: usize, path: &[[u8; 32]], root: [u8; 32]) -> bool {
        let mut hash = leaf;
        let mut i = index;
        for sib in path {
            let mut c = Vec::with_capacity(64);
            if i % 2 == 0 {
                c.extend_from_slice(&hash);
                c.extend_from_slice(sib);
            } else {
                c.extend_from_slice(sib);
                c.extend_from_slice(&hash);
            }
            hash = sha256d(&c);
            i /= 2;
        }
        hash == root
    }
}

pub fn merkle_root(leaves: &[[u8; 32]]) -> [u8; 32] {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    let mut hashes: Vec<[u8; 32]> = leaves.to_vec();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proof_roundtrip() {
        let mut t = NoteCommitmentTree::new();
        for i in 0u8..5 {
            t.append([i; 32]);
        }
        let root = t.root();
        let path = t.proof(3).unwrap();
        assert!(NoteCommitmentTree::verify_inclusion([3u8; 32], 3, &path, root));
        assert!(!NoteCommitmentTree::verify_inclusion([9u8; 32], 3, &path, root));
    }

    #[test]
    fn empty_root_is_zero() {
        assert_eq!(NoteCommitmentTree::new().root(), [0u8; 32]);
    }
}
