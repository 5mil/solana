//! Stem/fluff first-hop cover for compact bundles. Lab model, not a P2P stack.

use super::action::ActionBundle;
use crate::consensus::pow::sha256d;

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

    pub fn first_hop_is_not_origin(&self) -> bool {
        self.hops.first().map(|h| h != &self.origin).unwrap_or(false)
    }
}

pub fn fluff_payload(bundle: &ActionBundle) -> ActionBundle {
    bundle.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stem_hides_origin_from_first_hop_id() {
        let p = StemPath::from_secret(b"wallet-1", 3);
        assert!(p.first_hop_is_not_origin());
        assert_eq!(p.hops.len(), 3);
        assert_ne!(p.fluff_peer, p.origin);
    }
}
