//! Window-wide ImageOr. Binding residual is conservation. Dest not on the proof.

use super::auth::RangeProof;
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
    for p in parts { h.update(p); }
    let out = h.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&out);
    Scalar::from_bytes_mod_order_wide(&wide)
}

fn must_pt(b: &[u8; 32]) -> Option<RistrettoPoint> { try_point(b) }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImageOr {
    pub a_i: Vec<[u8; 32]>,
    pub a_t: Vec<[u8; 32]>,
    pub e: Vec<[u8; 32]>,
    pub z_sk: Vec<[u8; 32]>,
    pub z_d: Vec<[u8; 32]>,
}

impl ImageOr {
    pub fn prove(
        leaves: &[[u8; 32]], real: usize, sk: &SpendKey, delta: &Scalar,
        image: &[u8; 32], c_prime: &ValueCommitment, ctx: &[u8],
    ) -> Result<Self, &'static str> {
        let n = leaves.len();
        if n == 0 || real >= n { return Err("empty or bad index"); }
        let gens = PedersenGenerators::default();
        let mut a_i = vec![[0u8; 32]; n];
        let mut a_t = vec![[0u8; 32]; n];
        let mut e = vec![[0u8; 32]; n];
        let mut z_sk = vec![[0u8; 32]; n];
        let mut z_d = vec![[0u8; 32]; n];
        let k_sk = h_scalar(b"img-ksk-v4", &[&sk.sk.to_bytes(), ctx, image]);
        let k_d = h_scalar(b"img-kd-v4", &[&delta.to_bytes(), ctx, image]);
        for j in 0..n {
            let p_j = hash_to_point(b"hp-cm", &leaves[j]);
            if j == real {
                a_i[j] = (k_sk * p_j).compress().to_bytes();
                a_t[j] = (k_d * gens.h).compress().to_bytes();
            } else {
                let ej = h_scalar(b"img-sim-v4", &[&sk.sk.to_bytes(), &j.to_le_bytes(), ctx]);
                let zs = h_scalar(b"img-simsk-v4", &[&sk.sk.to_bytes(), &j.to_le_bytes(), ctx]);
                let zd = h_scalar(b"img-simd-v4", &[&sk.sk.to_bytes(), &j.to_le_bytes(), ctx]);
                let leaf_pt = must_pt(&leaves[j]).ok_or("non-canonical live leaf")?;
                let img_pt = must_pt(image).ok_or("non-canonical image")?;
                a_i[j] = (zs * p_j - ej * img_pt).compress().to_bytes();
                a_t[j] = (zd * gens.h - ej * (c_prime.point() - leaf_pt)).compress().to_bytes();
                e[j] = ej.to_bytes();
                z_sk[j] = zs.to_bytes();
                z_d[j] = zd.to_bytes();
            }
        }
        let chal = Self::challenge(leaves, image, c_prime, ctx, &a_i, &a_t);
        let mut e_real = chal;
        for j in 0..n {
            if j != real { e_real -= Scalar::from_bytes_mod_order(e[j]); }
        }
        e[real] = e_real.to_bytes();
        z_sk[real] = (k_sk + e_real * sk.sk).to_bytes();
        z_d[real] = (k_d + e_real * *delta).to_bytes();
        Ok(Self { a_i, a_t, e, z_sk, z_d })
    }

    fn challenge(
        leaves: &[[u8; 32]], image: &[u8; 32], c_prime: &ValueCommitment,
        ctx: &[u8], a_i: &[[u8; 32]], a_t: &[[u8; 32]],
    ) -> Scalar {
        let mut flat = Vec::new();
        flat.extend_from_slice(b"img-chal-v4");
        flat.extend_from_slice(ctx);
        flat.extend_from_slice(image);
        flat.extend_from_slice(&c_prime.commitment);
        for l in leaves { flat.extend_from_slice(l); }
        for a in a_i { flat.extend_from_slice(a); }
        for a in a_t { flat.extend_from_slice(a); }
        h_scalar(b"img-chal-v4", &[&flat])
    }

    pub fn verify(&self, leaves: &[[u8; 32]], image: &[u8; 32], c_prime: &ValueCommitment, ctx: &[u8]) -> bool {
        let n = leaves.len();
        if n == 0 || self.a_i.len() != n || self.a_t.len() != n || self.e.len() != n { return false; }
        if self.z_sk.len() != n || self.z_d.len() != n { return false; }
        let img = match must_pt(image) { Some(p) => p, None => return false };
        let gens = PedersenGenerators::default();
        let chal = Self::challenge(leaves, image, c_prime, ctx, &self.a_i, &self.a_t);
        let mut sum = Scalar::ZERO;
        for j in 0..n {
            let leaf_pt = match must_pt(&leaves[j]) { Some(p) => p, None => return false };
            let ai = match must_pt(&self.a_i[j]) { Some(p) => p, None => return false };
            let at = match must_pt(&self.a_t[j]) { Some(p) => p, None => return false };
            let ej = Scalar::from_bytes_mod_order(self.e[j]);
            let zs = Scalar::from_bytes_mod_order(self.z_sk[j]);
            let zd = Scalar::from_bytes_mod_order(self.z_d[j]);
            if zs * hash_to_point(b"hp-cm", &leaves[j]) != ai + ej * img { return false; }
            if zd * gens.h != at + ej * (c_prime.point() - leaf_pt) { return false; }
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
        cms: &[[u8; 32]], real: usize, reward: u64, height: u64, r_open: &Scalar, r_pad: &Scalar,
    ) -> Result<Self, &'static str> {
        let gens = PedersenGenerators::default();
        let n = cms.len();
        if n == 0 || real >= n { return Err("bad emission set"); }
        let target = Scalar::from(reward) * gens.g;
        let mut a = vec![[0u8; 32]; n];
        let mut e = vec![[0u8; 32]; n];
        let mut z = vec![[0u8; 32]; n];
        let k = h_scalar(b"emit-k-v4", &[&r_open.to_bytes(), &height.to_le_bytes()]);
        for j in 0..n {
            if j == real {
                a[j] = (k * gens.h).compress().to_bytes();
            } else {
                let ej = h_scalar(b"emit-sim-v4", &[&r_pad.to_bytes(), &j.to_le_bytes(), &height.to_le_bytes()]);
                let zj = h_scalar(b"emit-simz-v4", &[&r_pad.to_bytes(), &j.to_le_bytes(), &height.to_le_bytes()]);
                let leaf = must_pt(&cms[j]).ok_or("non-canonical emission cm")?;
                a[j] = (zj * gens.h - ej * (leaf - target)).compress().to_bytes();
                e[j] = ej.to_bytes();
                z[j] = zj.to_bytes();
            }
        }
        let mut flat = Vec::new();
        flat.extend_from_slice(b"emit-chal-v4");
        flat.extend_from_slice(&reward.to_le_bytes());
        flat.extend_from_slice(&height.to_le_bytes());
        for c in cms { flat.extend_from_slice(c); }
        for x in &a { flat.extend_from_slice(x); }
        let chal = h_scalar(b"emit-chal-v4", &[&flat]);
        let mut e_real = chal;
        for j in 0..n {
            if j != real { e_real -= Scalar::from_bytes_mod_order(e[j]); }
        }
        e[real] = e_real.to_bytes();
        z[real] = (k + e_real * *r_open).to_bytes();
        Ok(Self { a, e, z, reward, height })
    }

    pub fn verify(&self, cms: &[[u8; 32]], reward: u64, height: u64) -> bool {
        if self.reward != reward || self.height != height { return false; }
        let gens = PedersenGenerators::default();
        let n = cms.len();
        if self.a.len() != n || n == 0 { return false; }
        let target = Scalar::from(reward) * gens.g;
        let mut flat = Vec::new();
        flat.extend_from_slice(b"emit-chal-v4");
        flat.extend_from_slice(&reward.to_le_bytes());
        flat.extend_from_slice(&height.to_le_bytes());
        for c in cms { flat.extend_from_slice(c); }
        for x in &self.a { flat.extend_from_slice(x); }
        let chal = h_scalar(b"emit-chal-v4", &[&flat]);
        let mut sum = Scalar::ZERO;
        for j in 0..n {
            let ej = Scalar::from_bytes_mod_order(self.e[j]);
            let zj = Scalar::from_bytes_mod_order(self.z[j]);
            let leaf = match must_pt(&cms[j]) { Some(p) => p, None => return false };
            let aj = match must_pt(&self.a[j]) { Some(p) => p, None => return false };
            if zj * gens.h != aj + ej * (leaf - target) { return false; }
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
    pub fn sign(r_bind: &Scalar, transcript: &[u8]) -> Result<Self, &'static str> {
        if *r_bind == Scalar::ZERO { return Err("binding key must be non-zero"); }
        let gens = PedersenGenerators::default();
        let k = blinding_from_seed(&[b"bind-k-v4".as_ref(), &r_bind.to_bytes(), transcript].concat());
        let r_pt = k * gens.h;
        let e = h_scalar(b"bind-e-v4", &[&r_pt.compress().to_bytes(), transcript]);
        Ok(Self { r: r_pt.compress().to_bytes(), s: (k + e * r_bind).to_bytes() })
    }

    pub fn verify(
        &self, inputs: &[ValueCommitment], outputs: &[ValueCommitment],
        fee: &ValueCommitment, transcript: &[u8],
    ) -> bool {
        let r_pt = match must_pt(&self.r) { Some(p) => p, None => return false };
        let gens = PedersenGenerators::default();
        let mut pk = RistrettoPoint::identity();
        for c in inputs { pk += c.point(); }
        for c in outputs { pk -= c.point(); }
        pk -= fee.point();
        if pk == RistrettoPoint::identity() { return false; }
        let s = Scalar::from_bytes_mod_order(self.s);
        let e = h_scalar(b"bind-e-v4", &[&self.r, transcript]);
        s * gens.h == r_pt + e * pk
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct NoteProof {
    pub image: ImageOr,
    pub range_in: RangeProof,
    pub range_outs: Vec<RangeProof>,
    pub range_fee: RangeProof,
    pub binding: BindingSig,
}

impl NoteProof {
    pub fn verify(
        &self, image: &[u8; 32], c_prime: &ValueCommitment, outputs: &[ValueCommitment],
        fee: &ValueCommitment, launch: &LaunchSet, ctx: &[u8], transcript: &[u8],
    ) -> bool {
        if outputs.is_empty() || self.range_outs.len() != outputs.len() { return false; }
        if !self.image.verify(&launch.live_leaves(), image, c_prime, ctx) { return false; }
        if !self.range_in.verify(c_prime) { return false; }
        for (p, c) in self.range_outs.iter().zip(outputs.iter()) {
            if !p.verify(c) { return false; }
        }
        self.range_fee.verify(fee) && self.binding.verify(&[c_prime.clone()], outputs, fee, transcript)
    }
}

pub fn asset_scalar(asset: &[u8; 32]) -> Scalar {
    if *asset == [0u8; 32] { Scalar::ZERO } else { h_scalar(b"asset", &[asset]) }
}

pub fn commit_with_asset(value: u64, blinding: &Scalar, asset: &[u8; 32]) -> ValueCommitment {
    let gens = PedersenGenerators::default();
    let a = asset_scalar(asset);
    let point = Scalar::from(value) * gens.g + blinding * gens.h + a * gens.asset();
    ValueCommitment { commitment: point.compress().to_bytes() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::auth::rerand;
    use crate::notes::commitment::blinding_from_seed;
    use crate::notes::launch::LaunchSet;

    #[test]
    fn image_or_is_over_full_window_not_a_listed_subset() {
        let sk = SpendKey::from_wallet_seed(b"owner");
        let r = blinding_from_seed(b"r");
        let cm = ValueCommitment::commit(9, &r, &PedersenGenerators::default());
        let delta = blinding_from_seed(b"d");
        let c2 = rerand(&cm, &delta);
        let mut set = LaunchSet::standard();
        let decoy_a = ValueCommitment::commit(1, &blinding_from_seed(b"da"), &PedersenGenerators::default());
        let decoy_b = ValueCommitment::commit(2, &blinding_from_seed(b"db"), &PedersenGenerators::default());
        set.append(decoy_a.commitment);
        set.append(cm.commitment);
        set.append(decoy_b.commitment);
        let leaves = set.live_leaves();
        let idx = leaves.iter().position(|l| *l == cm.commitment).unwrap();
        let img = sk.key_image(&cm.commitment);
        let p = ImageOr::prove(&leaves, idx, &sk, &delta, &img, &c2, b"ctx").unwrap();
        assert!(p.verify(&leaves, &img, &c2, b"ctx"));
        assert!(!p.verify(&leaves, &img, &c2, b"other"));
        let thief = SpendKey::from_wallet_seed(b"thief").key_image(&cm.commitment);
        assert!(!p.verify(&leaves, &thief, &c2, b"ctx"));
        assert!(!format!("{:?}", p).contains("ring:"));
    }

    #[test]
    fn binding_rejects_zero_residual() {
        let gens = PedersenGenerators::default();
        let r = blinding_from_seed(b"same");
        let a = ValueCommitment::commit(5, &r, &gens);
        let b = ValueCommitment::commit(5, &r, &gens);
        let fee = ValueCommitment::identity();
        assert!(BindingSig::sign(&Scalar::ZERO, b"t").is_err());
        let sig = BindingSig::sign(&blinding_from_seed(b"nz"), b"t").unwrap();
        assert!(!sig.verify(&[a], &[b], &fee, b"t"));
    }
}
