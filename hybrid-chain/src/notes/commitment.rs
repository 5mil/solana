//! Pedersen value commitments over Ristretto. Asset uses a third generator.

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
    pub a: RistrettoPoint,
}

impl Default for PedersenGenerators {
    fn default() -> Self {
        let g = RISTRETTO_BASEPOINT_POINT;
        let h = point_from_label(b"hybrid-chain-pedersen-H");
        let a = point_from_label(b"hybrid-chain-pedersen-A");
        Self { g, h, a }
    }
}

impl PedersenGenerators {
    pub fn asset(&self) -> RistrettoPoint {
        self.a
    }
}

fn point_from_label(label: &[u8]) -> RistrettoPoint {
    let mut h = Sha512::new();
    h.update(label);
    let out = h.finalize();
    let mut uniform = [0u8; 64];
    uniform.copy_from_slice(&out);
    RistrettoPoint::from_uniform_bytes(&uniform)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueCommitment {
    pub commitment: [u8; 32],
}

impl ValueCommitment {
    pub fn commit(value: u64, blinding: &Scalar, gens: &PedersenGenerators) -> Self {
        let point = Scalar::from(value) * gens.g + blinding * gens.h;
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
}
