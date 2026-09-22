//! Construct a conserved transfer: one real spend + one real output + fee.

use super::action::{
    ActionBundle, CompactAction, CompactOutput, CompactSpend, BUNDLE_PAD,
};
use super::commitment::{PedersenGenerators, ValueCommitment};
use super::membership::MembershipProof;
use super::payout::discovery_tag;
use super::tags::SpendTagSet;
use super::tree::NoteCommitmentTree;
use curve25519_dalek::scalar::Scalar;

pub fn transfer_bundle(
    spend_secret: &[u8; 32],
    tree: &NoteCommitmentTree,
    leaf_index: usize,
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
    let proof = MembershipProof::prove(tree, leaf_index).ok_or("leaf not in tree")?;
    let gens = PedersenGenerators::default();
    let in_c = ValueCommitment::commit(in_value, in_blind, &gens);
    if in_c.commitment != proof.leaf {
        return Err("commitment does not match tree leaf");
    }
    if !proof.verify(tree.root()) {
        return Err("membership failed");
    }
    let tag = SpendTagSet::derive(spend_secret, &proof.leaf);
    let out_c = ValueCommitment::commit(out_value, out_blind, &gens);
    let fee_c = ValueCommitment::commit(fee_value, fee_blind, &gens);
    let bundle = ActionBundle {
        version: 1,
        actions: vec![CompactAction {
            spend: Some(CompactSpend {
                prev_txid: [0u8; 32],
                prev_vout: 0,
                spend_tag: tag,
                value_commitment: in_c,
                dummy: false,
                membership: Some(proof),
            }),
            output: Some(CompactOutput {
                one_time_dest: out_dest,
                discovery_tag: discovery_tag(out_scan, 0),
                value_commitment: out_c,
                dummy: false,
                asset_id,
            }),
        }],
        fee_commitment: fee_c,
        coinbase: false,
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
    use crate::notes::tree::NoteCommitmentTree;

    #[test]
    fn transfer_conserves_and_proves() {
        let gens = PedersenGenerators::default();
        let r_in = blinding_from_seed(b"in");
        let leaf = ValueCommitment::commit(100, &r_in, &gens).commitment;
        let mut tree = NoteCommitmentTree::new();
        tree.append(leaf);
        let r_out = blinding_from_seed(b"out");
        let r_fee = r_in - r_out;
        let b = transfer_bundle(
            &[9u8; 32],
            &tree,
            0,
            100,
            &r_in,
            [3u8; 32],
            b"scan",
            90,
            &r_out,
            10,
            &r_fee,
            [0u8; 32],
        )
        .expect("bundle");
        assert!(!b.coinbase);
        assert!(b.verify_conservation());
        assert_eq!(b.real_spends().len(), 1);
        assert_eq!(b.actions.len(), BUNDLE_PAD);
    }

    #[test]
    fn rejects_value_mismatch() {
        let gens = PedersenGenerators::default();
        let r = blinding_from_seed(b"in");
        let mut tree = NoteCommitmentTree::new();
        tree.append(ValueCommitment::commit(50, &r, &gens).commitment);
        let err = transfer_bundle(
            &[1u8; 32],
            &tree,
            0,
            50,
            &r,
            [1u8; 32],
            b"s",
            50,
            &r,
            10,
            &r,
            [0u8; 32],
        );
        assert!(err.is_err());
    }
}
