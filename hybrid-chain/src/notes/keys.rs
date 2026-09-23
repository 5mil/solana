//! Spend and scan keys. dest = sk·G. Tickets cannot derive sk.

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

fn hash_scalar(label: &[u8], parts: &[&[u8]]) -> Scalar {
    let mut h = Sha512::new();
    h.update(label);
    for p in parts {
        h.update(p);
    }
    let out = h.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&out);
    Scalar::from_bytes_mod_order_wide(&wide)
}

#[derive(Clone, Debug)]
pub struct SpendKey {
    pub sk: Scalar,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendPk {
    pub bytes: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct ScanKey {
    pub sk: Scalar,
}

impl SpendKey {
    /// Wallet seed — never a pool ticket / worker label.
    pub fn from_wallet_seed(seed: &[u8]) -> Self {
        Self {
            sk: hash_scalar(b"spend-sk", &[seed]),
        }
    }

    pub fn pk(&self) -> SpendPk {
        SpendPk {
            bytes: (self.sk * RISTRETTO_BASEPOINT_POINT).compress().to_bytes(),
        }
    }

    pub fn nullifier(&self, cm: &[u8; 32]) -> [u8; 32] {
        let mut h = Sha512::new();
        h.update(b"nf");
        h.update(self.sk.to_bytes());
        h.update(cm);
        let out = h.finalize();
        let mut tag = [0u8; 32];
        tag.copy_from_slice(&out[..32]);
        tag
    }
}

impl SpendPk {
    pub fn point(&self) -> RistrettoPoint {
        CompressedRistretto(self.bytes)
            .decompress()
            .unwrap_or(RISTRETTO_BASEPOINT_POINT)
    }
}

impl ScanKey {
    pub fn from_wallet_seed(seed: &[u8]) -> Self {
        Self {
            sk: hash_scalar(b"scan-sk", &[seed]),
        }
    }

    pub fn pk_bytes(&self) -> [u8; 32] {
        (self.sk * RISTRETTO_BASEPOINT_POINT).compress().to_bytes()
    }

    /// Diversified tag: ECDH(scan, eph) || diversifier. Never a fixed counter.
    pub fn tag(&self, eph_pk: &[u8; 32], diversifier: &[u8; 16]) -> [u8; 32] {
        let eph = CompressedRistretto(*eph_pk)
            .decompress()
            .unwrap_or(RISTRETTO_BASEPOINT_POINT);
        let shared = (self.sk * eph).compress().to_bytes();
        let mut h = Sha512::new();
        h.update(b"scan-tag");
        h.update(shared);
        h.update(diversifier);
        let out = h.finalize();
        let mut tag = [0u8; 32];
        tag.copy_from_slice(&out[..32]);
        tag
    }
}

/// note_id committed to the living window. Not the raw value commitment.
pub fn note_id(cm: &[u8; 32], dest: &SpendPk, asset_scalar: &Scalar) -> [u8; 32] {
    let mut h = Sha512::new();
    h.update(b"note-id");
    h.update(cm);
    h.update(dest.bytes);
    h.update(asset_scalar.to_bytes());
    let out = h.finalize();
    let mut id = [0u8; 32];
    id.copy_from_slice(&out[..32]);
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_string_is_not_the_spend_key() {
        let wallet = SpendKey::from_wallet_seed(b"wallet-seed-abc");
        let ticket = SpendKey::from_wallet_seed(b"miner");
        assert_ne!(wallet.pk().bytes, ticket.pk().bytes);
        assert_ne!(wallet.pk().bytes, *b"miner\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    }

    #[test]
    fn one_sk_one_cm_one_nf() {
        let sk = SpendKey::from_wallet_seed(b"w");
        let a = sk.nullifier(&[1u8; 32]);
        let b = sk.nullifier(&[1u8; 32]);
        let c = sk.nullifier(&[2u8; 32]);
        assert_eq!(a, b);
        assert_ne!(a, c);
        let other = SpendKey::from_wallet_seed(b"other");
        assert_ne!(a, other.nullifier(&[1u8; 32]));
    }
}
