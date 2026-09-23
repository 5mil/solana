//! Scan by diversified ECDH tag, not a reused counter.

use super::action::ActionBundle;
use super::keys::ScanKey;
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
    pub fn ingest_for_scan(&mut self, height: u64, bundle: &ActionBundle, scan: &ScanKey) {
        for out in bundle.real_outputs() {
            let tag = scan.tag(&out.eph_pk, &out.diversifier);
            self.entries.entry(tag).or_default().push(TagHit {
                height,
                dest: out.dest.bytes,
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
    use crate::notes::keys::ScanKey;
    use crate::notes::spend::emission_bundle;
    use crate::notes::keys::SpendKey;

    #[test]
    fn two_outputs_do_not_share_a_fixed_counter_tag() {
        let sk = SpendKey::from_wallet_seed(b"w");
        let scan = ScanKey::from_wallet_seed(b"s");
        let a = emission_bundle(&sk, &scan, 1, 5, [1u8; 16]);
        let b = emission_bundle(&sk, &scan, 1, 5, [2u8; 16]);
        let ta = scan.tag(&a.real_outputs()[0].eph_pk, &a.real_outputs()[0].diversifier);
        let tb = scan.tag(&b.real_outputs()[0].eph_pk, &b.real_outputs()[0].diversifier);
        assert_ne!(ta, tb);
    }
}
