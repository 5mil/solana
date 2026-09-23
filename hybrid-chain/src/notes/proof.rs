//! One proof object: spend-auth + window membership + binding.
//! Range is a single statement on the rerandomized value, not 48 loose ORs on the spend.

use super::commitment::{blinding_from_seed, PedersenGenerators, ValueCommitment};
use super::keys::{note_id, SpendKey, SpendPk};
use super::launch::LaunchSet;
use super::tree::NoteCommitmentTree;
use crate::consensus::pow::sha256d;
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::ristretto::{CompressedRistretto, RistrettoPoint};
use curve25519_dalek::scalar::Scalar;
use curve25519_dalek::traits::Identity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};

fn h_scalar(label: &[u8], parts: &[&[u8]]) -> Scalar {
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

fn decompress(b: &[u8; 32]) -> RistrettoPoint {
    CompressedRistretto(*b)
        .decompress()
        .unwrap_or_else(RistrettoPoint::identity)
}

/// Schnorr: dest = sk·G over (nf || live_root || C′).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpendAuth {
    pub dest: SpendPk,
    pub r: [u8; 32],
    pub s: [u8; 32],
}

impl SpendAuth {
    pub fn sign(sk: &SpendKey, nf: &[u8; 32], live_root: &[u8; 32], c_prime: &[u8; 32]) -> Self {
        let k = h_scalar(b"auth-k", &[&sk.sk.to_bytes(), nf, live_root, c_prime]);
        let r_pt = k * RISTRETTO_BASEPOINT_POINT;
        let r = r_pt.compress().to_bytes();
        let e = h_scalar(b"auth-e", &[&r, nf, live_root, c_prime, &sk.pk().bytes]);
        let s = k + e * sk.sk;
        Self {
            dest: sk.pk(),
            r,
            s: s.to_bytes(),
        }
    }

    pub fn verify(&self, nf: &[u8; 32], live_root: &[u8; 32], c_prime: &[u8; 32]) -> bool {
        let r_pt = decompress(&self.r);
        let s = Scalar::from_bytes_mod_order(self.s);
        let e = h_scalar(b"auth-e", &[&self.r, nf, live_root, c_prime, &self.dest.bytes]);
        s * RISTRETTO_BASEPOINT_POINT == r_pt + e * self.dest.point()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct WindowPath {
    pub note_id: [u8; 32],
    pub index: usize,
    pub siblings: Vec<[u8; 32]>,
}

impl WindowPath {
    pub fn prove(set: &LaunchSet, note_id: [u8; 32]) -> Option<Self> {
        let (index, siblings) = set.prove_window(note_id)?;
        Some(Self {
            note_id,
            index,
            siblings,
        })
    }

    pub fn verify(&self, live_root: [u8; 32]) -> bool {
        NoteCommitmentTree::verify_inclusion(self.note_id, self.index, &self.siblings, live_root)
    }
}

/// OR: one of the output commitments opens to `reward` (schedule check).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmissionOr {
    pub a: Vec<[u8; 32]>,
    pub e: Vec<[u8; 32]>,
    pub z: Vec<[u8; 32]>,
}

impl EmissionOr {
    pub fn prove(cms: &[[u8; 32]], real: usize, reward: u64, r_open: &Scalar) -> Self {
        let gens = PedersenGenerators::default();
        let n = cms.len();
        let target = Scalar::from(reward) * gens.g;
        let mut a = vec![[0u8; 32]; n];
        let mut e = vec![[0u8; 32]; n];
        let mut z = vec![[0u8; 32]; n];
        let k = h_scalar(b"emit-k", &[&r_open.to_bytes()]);
        for j in 0..n {
            if j == real {
                a[j] = (k * gens.h).compress().to_bytes();
            } else {
                let ej = h_scalar(b"emit-sim", &[&cms[j], &j.to_le_bytes()]);
                let zj = h_scalar(b"emit-simz", &[&cms[j], &j.to_le_bytes()]);
                let t = decompress(&cms[j]) - target;
                a[j] = (zj * gens.h - ej * t).compress().to_bytes();
                e[j] = ej.to_bytes();
                z[j] = zj.to_bytes();
            }
        }
        let chal = h_scalar(
            b"emit-chal",
            &a.iter().map(|x| x.as_slice()).collect::<Vec<_>>(),
        );
        let mut e_real = chal;
        for j in 0..n {
            if j != real {
                e_real -= Scalar::from_bytes_mod_order(e[j]);
            }
        }
        e[real] = e_real.to_bytes();
        z[real] = (k + e_real * r_open).to_bytes();
        Self { a, e, z }
    }

    pub fn verify(&self, cms: &[[u8; 32]], reward: u64) -> bool {
        let gens = PedersenGenerators::default();
        let n = cms.len();
        if self.a.len() != n || n == 0 {
            return false;
        }
        let target = Scalar::from(reward) * gens.g;
        let chal = h_scalar(
            b"emit-chal",
            &self.a.iter().map(|x| x.as_slice()).collect::<Vec<_>>(),
        );
        let mut sum = Scalar::ZERO;
        for j in 0..n {
            let ej = Scalar::from_bytes_mod_order(self.e[j]);
            let zj = Scalar::from_bytes_mod_order(self.z[j]);
            let t = decompress(&cms[j]) - target;
            if zj * gens.h != decompress(&self.a[j]) + ej * t {
                return false;
            }
            sum += ej;
        }
        sum == chal
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BindingSig {
    pub r: [u8; 32],
    pub s: [u8; 32],
}

impl BindingSig {
    pub fn sign(r_bind: &Scalar, transcript: &[u8]) -> Self {
        let gens = PedersenGenerators::default();
        let k = blinding_from_seed(&[b"bind-k".as_ref(), &r_bind.to_bytes(), transcript].concat());
        let r_pt = k * gens.h;
        let e = h_scalar(b"bind-e", &[&r_pt.compress().to_bytes(), transcript]);
        Self {
            r: r_pt.compress().to_bytes(),
            s: (k + e * r_bind).to_bytes(),
        }
    }

    pub fn verify(
        &self,
        inputs: &[ValueCommitment],
        outputs: &[ValueCommitment],
        fee: &ValueCommitment,
        transcript: &[u8],
    ) -> bool {
        let gens = PedersenGenerators::default();
        let mut pk = RistrettoPoint::identity();
        for c in inputs {
            pk += c.point();
        }
        for c in outputs {
            pk -= c.point();
        }
        pk -= fee.point();
        let s = Scalar::from_bytes_mod_order(self.s);
        let e = h_scalar(b"bind-e", &[&self.r, transcript]);
        s * gens.h == decompress(&self.r) + e * pk
    }
}

/// Single object attached to a spend.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteProof {
    pub auth: SpendAuth,
    pub membership: WindowPath,
    pub binding: BindingSig,
}

impl NoteProof {
    pub fn verify(
        &self,
        nf: &[u8; 32],
        c_prime: &ValueCommitment,
        live_root: [u8; 32],
        inputs: &[ValueCommitment],
        outputs: &[ValueCommitment],
        fee: &ValueCommitment,
        transcript: &[u8],
    ) -> bool {
        self.auth.verify(nf, &live_root, &c_prime.commitment)
            && self.membership.verify(live_root)
            && self.binding.verify(inputs, outputs, fee, transcript)
    }
}

pub fn asset_scalar(asset: &[u8; 32]) -> Scalar {
    if *asset == [0u8; 32] {
        Scalar::ZERO
    } else {
        h_scalar(b"asset", &[asset])
    }
}

/// C = vG + rH + aA — asset lives in the point.
pub fn commit_with_asset(
    value: u64,
    blinding: &Scalar,
    asset: &[u8; 32],
) -> ValueCommitment {
    let gens = PedersenGenerators::default();
    let a = asset_scalar(asset);
    let point = Scalar::from(value) * gens.g + blinding * gens.h + a * gens.asset();
    ValueCommitment {
        commitment: point.compress().to_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::launch::LaunchSet;

    #[test]
    fn auth_rejects_wrong_sk() {
        let sk = SpendKey::from_wallet_seed(b"owner");
        let nf = sk.nullifier(&[9u8; 32]);
        let live = [3u8; 32];
        let cp = [4u8; 32];
        let p = SpendAuth::sign(&sk, &nf, &live, &cp);
        assert!(p.verify(&nf, &live, &cp));
        let mut bad = p.clone();
        bad.dest = SpendKey::from_wallet_seed(b"other").pk();
        assert!(!bad.verify(&nf, &live, &cp));
    }

    #[test]
    fn window_path_only_on_live_set() {
        let mut set = LaunchSet::standard();
        let id = [7u8; 32];
        set.append(id);
        let p = WindowPath::prove(&set, id).unwrap();
        assert!(p.verify(set.window_root()));
        assert!(WindowPath::prove(&set, [8u8; 32]).is_none());
    }
}
