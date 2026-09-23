//! One real output. Binding residual is conservation. Dest never on the spend.

use super::action::{ActionBundle, CompactAction, CompactOutput, CompactSpend};
use super::auth::{rerand, RangeProof};
use super::commitment::{blinding_from_seed, PedersenGenerators, ValueCommitment};
use super::keys::{ScanKey, SpendKey};
use super::launch::LaunchSet;
use super::proof::{commit_with_asset, BindingSig, EmissionOr, ImageOr, NoteProof};
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::scalar::Scalar;

pub fn transfer_window_bundle(
    sk: &SpendKey,
    scan: &ScanKey,
    set: &LaunchSet,
    note_cm: [u8; 32],
    in_value: u64,
    in_blind: &Scalar,
    out_sk: &SpendKey,
    out_value: u64,
    out_blind: &Scalar,
    fee_value: u64,
    fee_blind: &Scalar,
    diversifier: [u8; 16],
    height: u64,
) -> Result<ActionBundle, &'static str> {
    if in_value != out_value.saturating_add(fee_value) {
        return Err("values do not conserve");
    }
    let gens = PedersenGenerators::default();
    let expected = ValueCommitment::commit(in_value, in_blind, &gens);
    if expected.commitment != note_cm {
        return Err("opening mismatch");
    }
    if !set.contains(note_cm) {
        return Err("note not in living window");
    }
    let leaves = set.live_leaves();
    let idx = leaves.iter().position(|l| *l == note_cm).ok_or("leaf missing")?;
    let delta = blinding_from_seed(&[b"rerand".as_ref(), &sk.sk.to_bytes(), &note_cm].concat());
    let c_prime = rerand(&expected, &delta);
    let image = sk.key_image(&note_cm);
    let mut ctx = Vec::new();
    ctx.extend_from_slice(&set.window_root());
    ctx.extend_from_slice(&height.to_le_bytes());
    let image_or = ImageOr::prove(&leaves, idx, sk, &delta, &image, &c_prime, &ctx)?;
    let out_cm = commit_with_asset(out_value, out_blind, &[0u8; 32]);
    let fee_c = ValueCommitment::commit(fee_value, fee_blind, &gens);
    let r_bind = (*in_blind + delta) - *out_blind - *fee_blind;
    let mut transcript = Vec::new();
    transcript.extend_from_slice(&image);
    transcript.extend_from_slice(&out_cm.commitment);
    transcript.extend_from_slice(&fee_c.commitment);
    transcript.extend_from_slice(&set.window_root());
    transcript.extend_from_slice(&height.to_le_bytes());
    let binding = BindingSig::sign(&r_bind, &transcript)?;
    let proof = NoteProof {
        image: image_or,
        range_in: RangeProof::prove(in_value, &(in_blind + delta)),
        range_outs: vec![RangeProof::prove(out_value, out_blind)],
        range_fee: RangeProof::prove(fee_value, fee_blind),
        binding: binding.clone(),
    };
    let eph_sk = blinding_from_seed(&[b"eph".as_ref(), &diversifier, &image].concat());
    let eph_pk = (eph_sk * RISTRETTO_BASEPOINT_POINT).compress().to_bytes();
    let _ = scan;
    let bundle = ActionBundle {
        version: 6,
        actions: vec![CompactAction {
            spend: Some(CompactSpend {
                spend_tag: image,
                rerand: c_prime,
                proof: Some(proof),
            }),
            output: Some(CompactOutput {
                dest: out_sk.pk(),
                eph_pk,
                diversifier,
                value_commitment: out_cm,
            }),
        }],
        fee_commitment: fee_c,
        binding: Some(binding),
        emission: None,
    };
    if !bundle.verify_conservation() {
        return Err("conservation failed");
    }
    Ok(bundle)
}

pub fn emission_bundle(
    dest_sk: &SpendKey,
    scan: &ScanKey,
    height: u64,
    reward: u64,
    diversifier: [u8; 16],
) -> ActionBundle {
    let r = blinding_from_seed(
        &[b"emit-r".as_ref(), &dest_sk.sk.to_bytes(), &height.to_le_bytes()].concat(),
    );
    let cm = commit_with_asset(reward, &r, &[0u8; 32]);
    let eph_sk = blinding_from_seed(&[b"eph-e".as_ref(), &diversifier, &dest_sk.pk().bytes].concat());
    let eph_pk = (eph_sk * RISTRETTO_BASEPOINT_POINT).compress().to_bytes();
    let real = CompactOutput {
        dest: dest_sk.pk(),
        eph_pk,
        diversifier,
        value_commitment: cm,
    };
    let pad_sk = SpendKey::from_wallet_seed(&[b"pad-from-open".as_ref(), &r.to_bytes()].concat());
    let pad_r = blinding_from_seed(&[b"emit-pad-r".as_ref(), &r.to_bytes()].concat());
    let pad_div = {
        let mut d = [0u8; 16];
        d.copy_from_slice(&blinding_from_seed(&[b"pad-d".as_ref(), &r.to_bytes()].concat()).to_bytes()[..16]);
        d
    };
    let pad_eph_sk = blinding_from_seed(&[b"pad-eph".as_ref(), &r.to_bytes()].concat());
    let pad = CompactOutput {
        dest: pad_sk.pk(),
        eph_pk: (pad_eph_sk * RISTRETTO_BASEPOINT_POINT).compress().to_bytes(),
        diversifier: pad_div,
        value_commitment: commit_with_asset(0, &pad_r, &[0u8; 32]),
    };
    let real_first = r.to_bytes()[0] & 1 == 0;
    let (o0, o1, real_idx) = if real_first {
        (real, pad, 0usize)
    } else {
        (pad, real, 1usize)
    };
    let cms = vec![o0.value_commitment.commitment, o1.value_commitment.commitment];
    let emission = EmissionOr::prove(&cms, real_idx, reward, height, &r, &pad_r).expect("emission or");
    let _ = scan;
    ActionBundle {
        version: 6,
        actions: vec![
            CompactAction { spend: None, output: Some(o0) },
            CompactAction { spend: None, output: Some(o1) },
        ],
        fee_commitment: ValueCommitment::identity(),
        binding: None,
        emission: Some(emission),
    }
}
