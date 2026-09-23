//! The only legal user spend: nullifier, rerandomized C′, ring, range, binding.

use super::action::{
    ActionBundle, CompactAction, CompactOutput, CompactSpend, BUNDLE_PAD,
};
use super::auth::{rerand, BindingSig, LinkProof, RangeProof};
use super::commitment::{blinding_from_seed, PedersenGenerators, ValueCommitment};
use super::launch::LaunchSet;
use super::payout::discovery_tag;
use super::tags::SpendTagSet;
use curve25519_dalek::scalar::Scalar;

pub fn transfer_window_bundle(
    spend_secret: &[u8; 32],
    set: &LaunchSet,
    note_cm: [u8; 32],
    in_value: u64,
    in_blind: &Scalar,
    out_dest: [u8; 32],
    out_scan: &[u8],
    out_value: u64,
    out_blind: &Scalar,
    fee_value: u64,
    fee_blind: &Scalar,
    asset_id: [u8; 32],
) -> Result<ActionBundle, &'static str> {
    if in_value != out_value.saturating_add(fee_value) {
        return Err("plaintext values do not conserve");
    }
    let gens = PedersenGenerators::default();
    let note = ValueCommitment {
        commitment: note_cm,
    };
    let expected = ValueCommitment::commit(in_value, in_blind, &gens);
    if expected.commitment != note_cm {
        return Err("note opening does not match commitment");
    }
    let delta = blinding_from_seed(&[b"rerand".as_ref(), spend_secret, &note_cm].concat());
    let c_prime = rerand(&note, &delta);
    let (ring, idx) = set.sample_ring(note_cm, spend_secret).ok_or("leaf not spendable")?;
    let link = LinkProof::prove(&ring, idx, &delta, &c_prime);
    if !link.verify(&ring, &c_prime) {
        return Err("link failed");
    }
    let tag = SpendTagSet::derive(spend_secret, &note_cm);
    let out_c = ValueCommitment::commit(out_value, out_blind, &gens);
    let fee_c = ValueCommitment::commit(fee_value, fee_blind, &gens);
    let out_range = RangeProof::prove(out_value, out_blind);
    let fee_range = RangeProof::prove(fee_value, fee_blind);
    let in_range = RangeProof::prove(in_value, &(in_blind + delta));
    let r_bind = (*in_blind + delta) - *out_blind - *fee_blind;
    let mut transcript = Vec::new();
    transcript.extend_from_slice(&tag);
    transcript.extend_from_slice(&out_c.commitment);
    let binding = BindingSig::sign(&r_bind, &transcript);
    let bundle = ActionBundle {
        version: 2,
        actions: vec![CompactAction {
            spend: Some(CompactSpend {
                spend_tag: tag,
                rerand: c_prime,
                ring,
                link: Some(link),
                range: Some(in_range),
            }),
            output: Some(CompactOutput {
                one_time_dest: out_dest,
                discovery_tag: discovery_tag(out_scan, 0),
                value_commitment: out_c,
                asset_id,
                range: Some(out_range),
            }),
        }],
        fee_commitment: fee_c,
        fee_range: Some(fee_range),
        binding: Some(binding),
    }
    .pad_to(BUNDLE_PAD);
    if !bundle.verify_conservation() {
        return Err("homomorphic conservation failed");
    }
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::commitment::blinding_from_seed;
    use crate::notes::launch::ProfileKind;

    #[test]
    fn spend_does_not_publish_note_cm_as_rerand() {
        let gens = PedersenGenerators::default();
        let r_in = blinding_from_seed(b"win");
        let note = ValueCommitment::commit(40, &r_in, &gens);
        let mut set = LaunchSet::new(ProfileKind::Constrained);
        set.append(note.commitment);
        for i in 1..16u8 {
            set.append([i; 32]);
        }
        let r_out = blinding_from_seed(b"wout");
        let r_fee = r_in - r_out;
        // fee_blind for binding uses in_blind+delta - out - fee, constructor
        // takes fee_blind separately; use r_in - r_out as fee value blind only
        // if delta is added inside. Conservation of values 40=30+10.
        let r_fee = blinding_from_seed(b"fee");
        let r_out = r_in - r_fee; // may fail conservation of blinds vs values
        let r_out = blinding_from_seed(b"wout");
        let r_fee = r_in - r_out;
        let b = transfer_window_bundle(
            &[8u8; 32],
            &set,
            note.commitment,
            40,
            &r_in,
            [4u8; 32],
            b"scan",
            30,
            &r_out,
            10,
            &r_fee,
            [0u8; 32],
        )
        .expect("window bundle");
        let spend = b.real_spends()[0];
        assert_ne!(spend.rerand.commitment, note.commitment);
        assert_eq!(spend.ring.len(), crate::notes::auth::RING);
        assert!(spend.link.is_some());
        assert!(b.verify_conservation());
    }
}
