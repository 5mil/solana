//! Pedersen value commitments over Ristretto.

use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

#[derive(Clone, Debug)]
pub struct PedersenGenerators {
    pub g: RistrettoPoint,
    pub h: RistrettoPoint,
}

impl Default for PedersenGenerators {
    fn default() -> Self {
        let g = RISTRETTO_BASEPOINT_POINT;
        let mut h_hash = Sha512::new();
        h_hash.update(b"hybrid-chain-pedersen-H");
        let out = h_hash.finalize();
        let mut uniform = [0u8; 64];
        uniform.copy_from_slice(&out);
        let h = RistrettoPoint::from_uniform_bytes(&uniform);
        Self { g, h }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueCommitment {
    pub commitment: [u8; 32],
}

impl ValueCommitment {
    pub fn commit(value: u64, blinding: &Scalar, gens: &PedersenGenerators) -> Self {
        let v = Scalar::from(value);
        let point = v * gens.g + blinding * gens.h;
        Self {
            commitment: point.compress().to_bytes(),
        }
    }

    pub fn identity() -> Self {
        Self {
            commitment: RistrettoPoint::identity().compress().to_bytes(),
        }
    }

    pub fn point(&self) -> RistrettoPoint {
        CompressedRistretto(self.commitment)
            .decompress()
            .unwrap_or_else(RistrettoPoint::identity)
    }

    pub fn add(&self, other: &Self) -> Self {
        Self {
            commitment: (self.point() + other.point()).compress().to_bytes(),
        }
    }

    pub fn sub(&self, other: &Self) -> Self {
        Self {
            commitment: (self.point() - other.point()).compress().to_bytes(),
        }
    }
}

/// Σ inputs == Σ outputs + fee (homomorphic).
/// Coinbase: inputs empty, fee may be identity, outputs commit to emission.
pub fn verify_balance(
    inputs: &[ValueCommitment],
    outputs: &[ValueCommitment],
    fee: &ValueCommitment,
) -> bool {
    let mut left = RistrettoPoint::identity();
    for c in inputs {
        left += c.point();
    }
    let mut right = fee.point();
    for c in outputs {
        right += c.point();
    }
    left == right
}

pub fn blinding_from_seed(seed: &[u8]) -> Scalar {
    let mut h = Sha512::new();
    h.update(b"blind");
    h.update(seed);
    let out = h.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&out);
    Scalar::from_bytes_mod_order_wide(&wide)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balance_holds() {
        let gens = PedersenGenerators::default();
        let r1 = blinding_from_seed(b"r1");
        let r2 = blinding_from_seed(b"r2");
        let r_out = blinding_from_seed(b"rout");
        let r_fee = r1 + r2 - r_out;
        let in1 = ValueCommitment::commit(50, &r1, &gens);
        let in2 = ValueCommitment::commit(50, &r2, &gens);
        let out = ValueCommitment::commit(90, &r_out, &gens);
        let fee = ValueCommitment::commit(10, &r_fee, &gens);
        assert!(verify_balance(&[in1, in2], &[out], &fee));
    }

    #[test]
    fn balance_rejects_mint() {
        let gens = PedersenGenerators::default();
        let r = blinding_from_seed(b"r");
        let inn = ValueCommitment::commit(50, &r, &gens);
        let out = ValueCommitment::commit(40, &r, &gens);
        let fee = ValueCommitment::commit(5, &r, &gens);
        assert!(!verify_balance(&[inn], &[out], &fee));
    }
}
