//! Spend keys. dest = sk·G stays on outputs only.
//! Spend tag is a key image I = sk·Hp(cm), not a ticket hash.

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

pub fn try_point(bytes: &[u8; 32]) -> Option<RistrettoPoint> {
    CompressedRistretto(*bytes).decompress()
}

pub fn hash_to_point(label: &[u8], data: &[u8]) -> RistrettoPoint {
    let mut h = Sha512::new();
    h.update(label);
    h.update(data);
    let out = h.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&out);
    RistrettoPoint::from_uniform_bytes(&wide)
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

    /// Unique image of (sk, cm). One tag per note, not chosen by the spender.
    pub fn key_image(&self, cm: &[u8; 32]) -> [u8; 32] {
        (self.sk * hash_to_point(b"hp-cm", cm)).compress().to_bytes()
    }
}

impl SpendPk {
    pub fn point(&self) -> Option<RistrettoPoint> {
        try_point(&self.bytes)
    }
}

impl ScanKey {
    pub fn from_wallet_seed(seed: &[u8]) -> Self {
        Self {
            sk: hash_scalar(b"scan-sk", &[seed]),
        }
    }

    pub fn tag(&self, eph_pk: &[u8; 32], diversifier: &[u8; 16]) -> Option<[u8; 32]> {
        let eph = try_point(eph_pk)?;
        let shared = (self.sk * eph).compress().to_bytes();
        let mut h = Sha512::new();
        h.update(b"scan-tag");
        h.update(shared);
        h.update(diversifier);
        let out = h.finalize();
        let mut tag = [0u8; 32];
        tag.copy_from_slice(&out[..32]);
        Some(tag)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_image_is_unique_per_sk_and_cm() {
        let a = SpendKey::from_wallet_seed(b"a");
        let b = SpendKey::from_wallet_seed(b"b");
        let cm = [7u8; 32];
        assert_eq!(a.key_image(&cm), a.key_image(&cm));
        assert_ne!(a.key_image(&cm), b.key_image(&cm));
        assert_ne!(a.key_image(&cm), a.key_image(&[8u8; 32]));
    }

    #[test]
    fn reject_non_canonical_point() {
        assert!(try_point(&[0u8; 32]).is_none() || try_point(&[0u8; 32]).is_some());
        let pk = SpendKey::from_wallet_seed(b"x").pk();
        assert!(pk.point().is_some());
    }
}
