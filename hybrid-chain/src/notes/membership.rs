//! Inclusion against the note tree. The wire object is a path, not a decoy list.

use super::tree::NoteCommitmentTree;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MembershipProof {
    pub leaf: [u8; 32],
    pub index: usize,
    pub path: Vec<[u8; 32]>,
}

impl MembershipProof {
    pub fn prove(tree: &NoteCommitmentTree, index: usize) -> Option<Self> {
        let leaf = *tree.leaves().get(index)?;
        let path = tree.proof(index)?;
        Some(Self { leaf, index, path })
    }

    pub fn verify(&self, root: [u8; 32]) -> bool {
        NoteCommitmentTree::verify_inclusion(self.leaf, self.index, &self.path, root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::tree::NoteCommitmentTree;

    #[test]
    fn proves_existing_leaf_only() {
        let mut t = NoteCommitmentTree::new();
        t.append([1u8; 32]);
        t.append([2u8; 32]);
        let p = MembershipProof::prove(&t, 1).unwrap();
        assert!(p.verify(t.root()));
        assert!(MembershipProof::prove(&t, 9).is_none());
    }
}
