//! NoteProof: key-image OR + range + binding. Dest is not a public field.

use super::auth::{RangeProof, RING};
use super::commitment::{blinding_from_seed, PedersenGenerators, ValueCommitment};
use super::keys::{hash_to_point, try_point, SpendKey};
use super::launch::LaunchSet;
use curve25519_dalek::ristretto::RistrettoPoint;
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

fn pt(b: &[u8; 32]) -> RistrettoPoint {
    try_point(b).unwrap_or_else(RistrettoPoint::identity)
}

/// OR: I = sk·Hp(cm_j) and C′ = cm_j + δH for exactly one window member.
/// Dest is not serialized.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageOr {
    pub ring: Vec<[u8; 32]>,
    pub a_i: Vec<[u8; 32]>,
    pub a_t: Vec<[u8; 32]>,
    pub e: Vec<[u8; 32]>,
    pub z_sk: Vec<[u8; 32]>,
    pub z_d: Vec<[u8; 32]>,
}

impl ImageOr {
    pub fn prove(
        ring: &[[u8; 32]],
        real: usize,
        sk: &SpendKey,
        delta: &Scalar,
        image: &[u8; 32],
        c_prime: &ValueCommitment,
        ctx: &[u8],
    ) -> Self {
        let gens = PedersenGenerators::default();
        let n = ring.len();
        let mut a_i = vec![[0u8; 32]; n];
        let mut a_t = vec![[0u8; 32]; n];
        let mut e = vec![[0u8; 32]; n];
        let mut z_sk = vec![[0u8; 32]; n];
        let mut z_d = vec![[0u8; 32]; n];
        let k_sk = h_scalar(b"img-ksk", &[&sk.sk.to_bytes(), ctx]);
        let k_d = h_scalar(b"img-kd", &[&delta.to_bytes(), ctx]);
        for j in 0..n {
            let p_j = hash_to_point(b"hp-cm", &ring[j]);
            if j == real {
                a_i[j] = (k_sk * p_j).compress().to_bytes();
                a_t[j] = (k_d * gens.h).compress().to_bytes();
            } else {
                let ej = h_scalar(b"img-sim", &[&ring[j], &j.to_le_bytes(), ctx]);
                let zs = h_scalar(b"img-simsk", &[&ring[j], &j.to_le_bytes(), ctx]);
                let zd = h_scalar(b"img-simd", &[&ring[j], &j.to_le_bytes(), ctx]);
                let t = c_prime.point() - pt(&ring[j]);
                a_i[j] = (zs * p_j - ej * pt(image)).compress().to_bytes();
                a_t[j] = (zd * gens.h - ej * t).compress().to_bytes();
                e[j] = ej.to_bytes();
                z_sk[j] = zs.to_bytes();
                z_d[j] = zd.to_bytes();
            }
        }
        let mut flat = Vec::new();
        flat.extend_from_slice(ctx);
        flat.extend_from_slice(image);
        flat.extend_from_slice(&c_prime.commitment);
        for r in ring {
            flat.extend_from_slice(r);
        }
        for a in &a_i {
            flat.extend_from_slice(a);
        }
        for a in &a_t {
            flat.extend_from_slice(a);
        }
        let chal = h_scalar(b"img-chal-v2", &[&flat]);
        let mut e_real = chal;
        for j in 0..n {
            if j != real {
                e_real -= Scalar::from_bytes_mod_order(e[j]);
            }
        }
        e[real] = e_real.to_bytes();
        z_sk[real] = (k_sk + e_real * sk.sk).to_bytes();
        z_d[real] = (k_d + e_real * delta).to_bytes();
        Self {
            ring: ring.to_vec(),
            a_i,
            a_t,
            e,
            z_sk,
            z_d,
        }
    }

    pub fn verify(&self, image: &[u8; 32], c_prime: &ValueCommitment, ctx: &[u8]) -> bool {
        let n = self.ring.len();
        if n == 0 || n > RING * 2 {
            return false;
        }
        if self.a_i.len() != n || self.e.len() != n {
            return false;
        }
        let gens = PedersenGenerators::default();
        let mut flat = Vec::new();
        flat.extend_from_slice(ctx);
        flat.extend_from_slice(image);
        flat.extend_from_slice(&c_prime.commitment);
        for r in &self.ring {
            flat.extend_from_slice(r);
        }
        for a in &self.a_i {
            flat.extend_from_slice(a);
        }
        for a in &self.a_t {
            flat.extend_from_slice(a);
        }
        let chal = h_scalar(b"img-chal-v2", &[&flat]);
        let mut sum = Scalar::ZERO;
        let img = match try_point(image) {
            Some(p) => p,
            None => return false,
        };
        for j in 0..n {
            let p_j = hash_to_point(b"hp-cm", &self.ring[j]);
            let ej = Scalar::from_bytes_mod_order(self.e[j]);
            let zs = Scalar::from_bytes_mod_order(self.z_sk[j]);
            let zd = Scalar::from_bytes_mod_order(self.z_d[j]);
            let t = c_prime.point() - pt(&self.ring[j]);
            if zs * p_j != pt(&self.a_i[j]) + ej * img {
                return false;
            }
            if zd * gens.h != pt(&self.a_t[j]) + ej * t {
                return false;
            }
            sum += ej;
        }
        sum == chal
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmissionOr {
    pub a: Vec<[u8; 32]>,
    pub e: Vec<[u8; 32]>,
    pub z: Vec<[u8; 32]>,
    pub reward: u64,
    pub height: u64,
}

impl EmissionOr {
    pub fn prove(
        cms: &[[u8; 32]],
        real: usize,
        reward: u64,
        height: u64,
        r_open: &Scalar,
    ) -> Self {
        let gens = PedersenGenerators::default();
        let n = cms.len();
        let target = Scalar::from(reward) * gens.g;
        let mut a = vec![[0u8; 32]; n];
        let mut e = vec![[0u8; 32]; n];
        let mut z = vec![[0u8; 32]; n];
        let k = h_scalar(b"emit-k", &[&r_open.to_bytes(), &height.to_le_bytes()]);
        for j in 0..n {
            if j == real {
                a[j] = (k * gens.h).compress().to_bytes();
            } else {
                let ej = h_scalar(
                    b"emit-sim",
                    &[&cms[j], &j.to_le_bytes(), &reward.to_le_bytes(), &height.to_le_bytes()],
                );
                let zj = h_scalar(
                    b"emit-simz",
                    &[&cms[j], &j.to_le_bytes(), &reward.to_le_bytes(), &height.to_le_bytes()],
                );
                let t = pt(&cms[j]) - target;
                a[j] = (zj * gens.h - ej * t).compress().to_bytes();
                e[j] = ej.to_bytes();
                z[j] = zj.to_bytes();
            }
        }
        let mut flat = Vec::new();
        flat.extend_from_slice(&reward.to_le_bytes());
        flat.extend_from_slice(&height.to_le_bytes());
        for c in cms {
            flat.extend_from_slice(c);
        }
        for x in &a {
            flat.extend_from_slice(x);
        }
        let chal = h_scalar(b"emit-chal-v2", &[&flat]);
        let mut e_real = chal;
        for j in 0..n {
            if j != real {
                e_real -= Scalar::from_bytes_mod_order(e[j]);
            }
        }
        e[real] = e_real.to_bytes();
        z[real] = (k + e_real * r_open).to_bytes();
        Self {
            a,
            e,
            z,
            reward,
            height,
        }
    }

    pub fn verify(&self, cms: &[[u8; 32]], reward: u64, height: u64) -> bool {
        if self.reward != reward || self.height != height {
            return false;
        }
        let gens = PedersenGenerators::default();
        let n = cms.len();
        if self.a.len() != n || n == 0 {
            return false;
        }
        let target = Scalar::from(reward) * gens.g;
        let mut flat = Vec::new();
        flat.extend_from_slice(&reward.to_le_bytes());
        flat.extend_from_slice(&height.to_le_bytes());
        for c in cms {
            flat.extend_from_slice(c);
        }
        for x in &self.a {
            flat.extend_from_slice(x);
        }
        let chal = h_scalar(b"emit-chal-v2", &[&flat]);
        let mut sum = Scalar::ZERO;
        for j in 0..n {
            let ej = Scalar::from_bytes_mod_order(self.e[j]);
            let zj = Scalar::from_bytes_mod_order(self.z[j]);
            let t = pt(&cms[j]) - target;
            if zj * gens.h != pt(&self.a[j]) + ej * t {
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
        let e = h_scalar(b"bind-e-v2", &[&r_pt.compress().to_bytes(), transcript]);
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
        let e = h_scalar(b"bind-e-v2", &[&self.r, transcript]);
        s * gens.h == pt(&self.r) + e * pk
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteProof {
    pub image: ImageOr,
    pub range_in: RangeProof,
    pub range_out: RangeProof,
    pub range_fee: RangeProof,
    pub binding: BindingSig,
}

impl NoteProof {
    pub fn verify(
        &self,
        image: &[u8; 32],
        c_prime: &ValueCommitment,
        out: &ValueCommitment,
        fee: &ValueCommitment,
        launch: &LaunchSet,
        ctx: &[u8],
        transcript: &[u8],
    ) -> bool {
        if !self.image.verify(image, c_prime, ctx) {
            return false;
        }
        for cm in &self.image.ring {
            if !launch.contains(*cm) {
                return false;
            }
        }
        self.range_in.verify(c_prime)
            && self.range_out.verify(out)
            && self.range_fee.verify(fee)
            && self.binding.verify(&[c_prime.clone()], &[out.clone()], fee, transcript)
    }
}

pub fn asset_scalar(asset: &[u8; 32]) -> Scalar {
    if *asset == [0u8; 32] {
        Scalar::ZERO
    } else {
        h_scalar(b"asset", &[asset])
    }
}

pub fn commit_with_asset(value: u64, blinding: &Scalar, asset: &[u8; 32]) -> ValueCommitment {
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
    use crate::notes::auth::rerand;
    use crate::notes::commitment::blinding_from_seed;

    #[test]
    fn image_or_hides_index_and_rejects_wrong_image() {
        let sk = SpendKey::from_wallet_seed(b"owner");
        let r = blinding_from_seed(b"r");
        let cm = ValueCommitment::commit(9, &r, &PedersenGenerators::default());
        let delta = blinding_from_seed(b"d");
        let c2 = rerand(&cm, &delta);
        let mut ring = vec![[3u8; 32]; 8];
        ring[2] = cm.commitment;
        let img = sk.key_image(&cm.commitment);
        let p = ImageOr::prove(&ring, 2, &sk, &delta, &img, &c2, b"ctx");
        assert!(p.verify(&img, &c2, b"ctx"));
        assert!(!p.verify(&img, &c2, b"other-ctx"));
        let other = SpendKey::from_wallet_seed(b"thief").key_image(&cm.commitment);
        assert!(!p.verify(&other, &c2, b"ctx"));
    }
}
