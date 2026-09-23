//! Legal spend: owner sk, window membership, binding. No listed ring.

use super::action::{
    ActionBundle, CompactAction, CompactOutput, CompactSpend, BUNDLE_PAD,
};
use super::auth::rerand;
use super::commitment::{blinding_from_seed, PedersenGenerators, ValueCommitment};
use super::keys::{note_id, ScanKey, SpendKey};
use super::launch::LaunchSet;
use super::proof::{
    asset_scalar, commit_with_asset, BindingSig, EmissionOr, NoteProof, SpendAuth, WindowPath,
};
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
) -> Result<ActionBundle, &'static str> {
    if in_value != out_value.saturating_add(fee_value) {
        return Err("values do not conserve");
    }
    let gens = PedersenGenerators::default();
    let expected = ValueCommitment::commit(in_value, in_blind, &gens);
    if expected.commitment != note_cm {
        return Err("opening mismatch");
    }
    let nid = note_id(&note_cm, &sk.pk(), &asset_scalar(&[0u8; 32]));
    if !set.contains(nid) {
        return Err("note not in living window");
    }
    let membership = WindowPath::prove(set, nid).ok_or("no window path")?;
    let delta = blinding_from_seed(&[b"rerand".as_ref(), &sk.sk.to_bytes(), &note_cm].concat());
    let c_prime = rerand(&expected, &delta);
    let nf = sk.nullifier(&note_cm);
    let live = set.window_root();
    let auth = SpendAuth::sign(sk, &nf, &live, &c_prime.commitment);
    if !auth.verify(&nf, &live, &c_prime.commitment) {
        return Err("auth failed");
    }
    let out_cm = commit_with_asset(out_value, out_blind, &[0u8; 32]);
    let fee_c = ValueCommitment::commit(fee_value, fee_blind, &gens);
    let r_bind = (*in_blind + delta) - *out_blind - *fee_blind;
    let mut transcript = Vec::new();
    transcript.extend_from_slice(&nf);
    transcript.extend_from_slice(&out_cm.commitment);
    let binding = BindingSig::sign(&r_bind, &transcript);
    let proof = NoteProof {
        auth,
        membership,
        binding: binding.clone(),
    };
    let eph_sk = blinding_from_seed(&[b"eph".as_ref(), &diversifier].concat());
    let eph_pk = (eph_sk * RISTRETTO_BASEPOINT_POINT).compress().to_bytes();
    let _tag = scan.tag(&eph_pk, &diversifier);
    let bundle = ActionBundle {
        version: 3,
        actions: vec![CompactAction {
            spend: Some(CompactSpend {
                spend_tag: nf,
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
    }
    .pad_to(BUNDLE_PAD);
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
    let r = blinding_from_seed(&[b"emit-r".as_ref(), &dest_sk.sk.to_bytes(), &height.to_le_bytes()].concat());
    let cm = commit_with_asset(reward, &r, &[0u8; 32]);
    let eph_sk = blinding_from_seed(&[b"eph-e".as_ref(), &diversifier].concat());
    let eph_pk = (eph_sk * RISTRETTO_BASEPOINT_POINT).compress().to_bytes();
    let real = CompactOutput {
        dest: dest_sk.pk(),
        eph_pk,
        diversifier,
        value_commitment: cm.clone(),
    };
    let pad_r = blinding_from_seed(&[b"emit-pad".as_ref(), &height.to_le_bytes()].concat());
    let pad_sk = SpendKey::from_wallet_seed(&[b"pad-dest".as_ref(), &height.to_le_bytes()].concat());
    let pad = CompactOutput {
        dest: pad_sk.pk(),
        eph_pk: [1u8; 32],
        diversifier: [2u8; 16],
        value_commitment: commit_with_asset(0, &pad_r, &[0u8; 32]),
    };
    let cms = vec![real.value_commitment.commitment, pad.value_commitment.commitment];
    let emission = EmissionOr::prove(&cms, 0, reward, &r);
    let _ = scan;
    ActionBundle {
        version: 3,
        actions: vec![
            CompactAction {
                spend: None,
                output: Some(real),
            },
            CompactAction {
                spend: None,
                output: Some(pad),
            },
        ],
        fee_commitment: ValueCommitment::identity(),
        binding: None,
        emission: Some(emission),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::launch::LaunchSet;

    #[test]
    fn owner_sk_required_and_cm_not_rerand() {
        let sk = SpendKey::from_wallet_seed(b"owner");
        let scan = ScanKey::from_wallet_seed(b"scan");
        let r_in = blinding_from_seed(b"in");
        let cm = ValueCommitment::commit(40, &r_in, &PedersenGenerators::default()).commitment;
        let nid = note_id(&cm, &sk.pk(), &asset_scalar(&[0u8; 32]));
        let mut set = LaunchSet::standard();
        set.append(nid);
        let out_sk = SpendKey::from_wallet_seed(b"recv");
        let r_out = blinding_from_seed(b"out");
        let r_fee = r_in - r_out;
        let b = transfer_window_bundle(
            &sk, &scan, &set, cm, 40, &r_in, &out_sk, 30, &r_out, 10, &r_fee, [3u8; 16],
        )
        .expect("bundle");
        let spend = b.real_spends()[0];
        assert_ne!(spend.rerand.commitment, cm);
        assert!(spend.proof.is_some());
        assert!(b.emission.is_none());
    }

    #[test]
    fn emission_or_does_not_use_height_only_point() {
        let sk = SpendKey::from_wallet_seed(b"miner-wallet");
        let scan = ScanKey::from_wallet_seed(b"scan");
        let a = emission_bundle(&sk, &scan, 1, 50, [1u8; 16]);
        let b = emission_bundle(&sk, &scan, 1, 50, [1u8; 16]);
        // same wallet+height is deterministic for tests; different height differs
        let c = emission_bundle(&sk, &scan, 2, 50, [1u8; 16]);
        assert_ne!(
            a.real_outputs()[0].value_commitment.commitment,
            c.real_outputs()[0].value_commitment.commitment
        );
        let cms: Vec<[u8; 32]> = a
            .actions
            .iter()
            .filter_map(|x| x.output.as_ref())
            .map(|o| o.value_commitment.commitment)
            .collect();
        assert!(a.emission.as_ref().unwrap().verify(&cms, 50));
        let _ = b;
    }
}
