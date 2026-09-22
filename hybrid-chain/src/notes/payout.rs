//! Sealed miner payout: dest is derived, not the worker label.

use crate::consensus::pow::sha256d;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SealedPayout {
    pub dest: [u8; 32],
    pub scan_seed: [u8; 32],
}

impl SealedPayout {
    pub fn from_ticket(ticket: &[u8], height: u64) -> Self {
        let mut scan_buf = Vec::new();
        scan_buf.extend_from_slice(b"scan-seed");
        scan_buf.extend_from_slice(ticket);
        scan_buf.extend_from_slice(&height.to_le_bytes());
        let scan_seed = sha256d(&scan_buf);
        let mut dest_buf = Vec::new();
        dest_buf.extend_from_slice(b"one-time-dest");
        dest_buf.extend_from_slice(&scan_seed);
        dest_buf.extend_from_slice(&height.to_le_bytes());
        let dest = sha256d(&dest_buf);
        Self { dest, scan_seed }
    }

    pub fn spend_secret(&self) -> [u8; 32] {
        sha256d(&[b"spend-seed".as_ref(), &self.scan_seed[..]].concat())
    }
}

pub fn discovery_tag(shared: &[u8], counter: u64) -> [u8; 32] {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"note-tag");
    buf.extend_from_slice(shared);
    buf.extend_from_slice(&counter.to_le_bytes());
    sha256d(&buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dest_is_not_the_ticket() {
        let p = SealedPayout::from_ticket(b"worker-17", 1);
        assert_ne!(&p.dest[..], b"worker-17");
        assert_ne!(p.dest, [0u8; 32]);
        let p2 = SealedPayout::from_ticket(b"worker-17", 1);
        assert_eq!(p, p2);
        let p3 = SealedPayout::from_ticket(b"worker-17", 2);
        assert_ne!(p.dest, p3.dest);
    }
}
