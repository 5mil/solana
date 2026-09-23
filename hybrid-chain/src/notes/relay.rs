//! In-process stem/fluff relay. Bundles move hop-by-hop before fluff.

use super::action::ActionBundle;
use crate::consensus::pow::sha256d;
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug)]
pub struct StemPath {
    pub origin: [u8; 32],
    pub hops: Vec<[u8; 32]>,
    pub fluff_peer: [u8; 32],
}

impl StemPath {
    pub fn from_secret(secret: &[u8], hops: usize) -> Self {
        let origin = sha256d(&[b"origin".as_ref(), secret].concat());
        let mut hops_out = Vec::new();
        let mut cur = origin;
        for i in 0..hops.max(1) {
            cur = sha256d(&[&cur[..], &i.to_le_bytes()].concat());
            hops_out.push(cur);
        }
        let fluff_peer = *hops_out.last().unwrap();
        Self {
            origin,
            hops: hops_out,
            fluff_peer,
        }
    }
}

#[derive(Clone, Debug)]
struct StemItem {
    bundle: ActionBundle,
    hop: usize,
    path: StemPath,
}

#[derive(Clone, Debug, Default)]
pub struct RelayNet {
    stem: VecDeque<StemItem>,
    fluff: Vec<ActionBundle>,
    seen: HashMap<[u8; 32], ()>,
}

fn bundle_id(bundle: &ActionBundle) -> [u8; 32] {
    sha256d(&bincode::serialize(bundle).unwrap_or_default())
}

impl RelayNet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest_stem(&mut self, secret: &[u8], bundle: ActionBundle, hops: usize) {
        let id = bundle_id(&bundle);
        if self.seen.contains_key(&id) {
            return;
        }
        self.seen.insert(id, ());
        self.stem.push_back(StemItem {
            bundle,
            hop: 0,
            path: StemPath::from_secret(secret, hops),
        });
    }

    /// Advance one hop. After the stem, the bundle is fluffed (broadcast).
    pub fn tick(&mut self) -> Vec<ActionBundle> {
        let mut fluffed = Vec::new();
        let mut next = VecDeque::new();
        while let Some(mut item) = self.stem.pop_front() {
            item.hop += 1;
            if item.hop >= item.path.hops.len() {
                self.fluff.push(item.bundle.clone());
                fluffed.push(item.bundle);
            } else {
                next.push_back(item);
            }
        }
        self.stem = next;
        fluffed
    }

    pub fn fluff_pool(&self) -> &[ActionBundle] {
        &self.fluff
    }

    pub fn stemming(&self) -> usize {
        self.stem.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::action::coinbase_bundle;
    use crate::notes::action::{emission_blinding, emission_commitment};
    use crate::notes::auth::RangeProof;

    #[test]
    fn stem_then_fluff_does_not_expose_origin_as_first_peer() {
        let dest = [1u8; 32];
        let r = emission_blinding(&dest, 0);
        let c = emission_commitment(&dest, 0, 5);
        let b = coinbase_bundle(dest, b"s", c, [0u8; 32], RangeProof::prove(5, &r));
        let path = StemPath::from_secret(b"wallet-1", 3);
        assert_ne!(path.hops[0], path.origin);
        let mut net = RelayNet::new();
        net.ingest_stem(b"wallet-1", b, 3);
        assert_eq!(net.stemming(), 1);
        assert!(net.fluff_pool().is_empty());
        assert!(net.tick().is_empty());
        assert!(net.tick().is_empty());
        let out = net.tick();
        assert_eq!(out.len(), 1);
        assert_eq!(net.fluff_pool().len(), 1);
    }
}
