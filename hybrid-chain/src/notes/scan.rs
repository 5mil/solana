//! Compact scan: index discovery tags, never a view key.

use super::action::ActionBundle;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TagIndex {
    entries: HashMap<[u8; 32], Vec<TagHit>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TagHit {
    pub height: u64,
    pub dest: [u8; 32],
    pub commitment: [u8; 32],
}

impl TagIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest(&mut self, height: u64, bundle: &ActionBundle) {
        for out in bundle.real_outputs() {
            self.entries.entry(out.discovery_tag).or_default().push(TagHit {
                height,
                dest: out.one_time_dest,
                commitment: out.value_commitment.commitment,
            });
        }
    }

    pub fn lookup(&self, tag: &[u8; 32]) -> &[TagHit] {
        self.entries.get(tag).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::action::{coinbase_bundle, emission_blinding, emission_commitment};
    use crate::notes::auth::RangeProof;
    use crate::notes::payout::discovery_tag;

    #[test]
    fn finds_own_tag_only() {
        let dest = [7u8; 32];
        let r = emission_blinding(&dest, 1);
        let c = emission_commitment(&dest, 1, 1);
        let b = coinbase_bundle(dest, b"scan-seed", c, [0u8; 32], RangeProof::prove(1, &r));
        let mut idx = TagIndex::new();
        idx.ingest(1, &b);
        let mine = discovery_tag(b"scan-seed", 0);
        assert_eq!(idx.lookup(&mine).len(), 1);
        assert!(idx.lookup(&[0u8; 32]).is_empty());
    }
}
