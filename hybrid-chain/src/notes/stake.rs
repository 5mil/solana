//! Stake eligibility as a note statement. Stake size stays in a commitment.

use super::commitment::{PedersenGenerators, ValueCommitment};
use super::membership::MembershipProof;
use super::tree::NoteCommitmentTree;
use crate::consensus::pos::validate_stake;
use curve25519_dalek::scalar::Scalar;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StakeProof {
    pub membership: MembershipProof,
    pub stake_commitment: ValueCommitment,
    pub seconds_held: u64,
}

impl StakeProof {
    pub fn create(
        tree: &NoteCommitmentTree,
        leaf_index: usize,
        coins: u64,
        blinding: &Scalar,
        seconds_held: u64,
    ) -> Result<Self, &'static str> {
        validate_stake(coins, seconds_held)?;
        let membership = MembershipProof::prove(tree, leaf_index).ok_or("stake leaf missing")?;
        let gens = PedersenGenerators::default();
        let stake_commitment = ValueCommitment::commit(coins, blinding, &gens);
        if stake_commitment.commitment != membership.leaf {
            return Err("stake commitment is not the tree leaf");
        }
        if !membership.verify(tree.root()) {
            return Err("stake membership failed");
        }
        Ok(Self {
            membership,
            stake_commitment,
            seconds_held,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::commitment::blinding_from_seed;
    use crate::params::CHAIN_PARAMS;

    #[test]
    fn stake_note_does_not_publish_coins_in_proof_fields_besides_commitment() {
        let r = blinding_from_seed(b"stake");
        let gens = PedersenGenerators::default();
        let coins = CHAIN_PARAMS.min_stake;
        let c = ValueCommitment::commit(coins, &r, &gens);
        let mut tree = NoteCommitmentTree::new();
        tree.append(c.commitment);
        let p = StakeProof::create(&tree, 0, coins, &r, CHAIN_PARAMS.pos_coin_age_min + 3600)
            .expect("ok");
        assert_eq!(p.stake_commitment, c);
        assert!(p.membership.verify(tree.root()));
    }
}
