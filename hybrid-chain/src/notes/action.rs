//! Dummy-padded compact actions. Conservation is required except coinbase.

use super::commitment::{verify_balance, ValueCommitment};
use super::payout::discovery_tag;
use serde::{Deserialize, Serialize};

pub const BUNDLE_PAD: usize = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactSpend {
    pub prev_txid: [u8; 32],
    pub prev_vout: u32,
    pub spend_tag: [u8; 32],
    pub value_commitment: ValueCommitment,
    pub dummy: bool,
    pub membership: Option<crate::notes::membership::MembershipProof>,
    pub hidden: Option<crate::notes::launch::HiddenProof>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactOutput {
    pub one_time_dest: [u8; 32],
    pub discovery_tag: [u8; 32],
    pub value_commitment: ValueCommitment,
    pub dummy: bool,
    pub asset_id: [u8; 32],
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactAction {
    pub spend: Option<CompactSpend>,
    pub output: Option<CompactOutput>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionBundle {
    pub version: u32,
    pub actions: Vec<CompactAction>,
    pub fee_commitment: ValueCommitment,
    pub coinbase: bool,
}

impl ActionBundle {
    pub fn pad_to(mut self, n: usize) -> Self {
        while self.actions.len() < n {
            self.actions.push(CompactAction {
                spend: Some(CompactSpend {
                    prev_txid: [0u8; 32],
                    prev_vout: 0,
                    spend_tag: [0u8; 32],
                    value_commitment: ValueCommitment::identity(),
                    dummy: true,
                    membership: None,
                    hidden: None,
                }),
                output: Some(CompactOutput {
                    one_time_dest: [0u8; 32],
                    discovery_tag: [0u8; 32],
                    value_commitment: ValueCommitment::identity(),
                    dummy: true,
                    asset_id: [0u8; 32],
                }),
            });
        }
        self
    }

    pub fn real_outputs(&self) -> Vec<&CompactOutput> {
        self.actions
            .iter()
            .filter_map(|a| a.output.as_ref())
            .filter(|o| !o.dummy)
            .collect()
    }

    pub fn real_spends(&self) -> Vec<&CompactSpend> {
        self.actions
            .iter()
            .filter_map(|a| a.spend.as_ref())
            .filter(|s| !s.dummy)
            .collect()
    }

    pub fn verify_conservation(&self) -> bool {
        let inputs: Vec<ValueCommitment> = self
            .real_spends()
            .into_iter()
            .map(|s| s.value_commitment.clone())
            .collect();
        let outputs: Vec<ValueCommitment> = self
            .real_outputs()
            .into_iter()
            .map(|o| o.value_commitment.clone())
            .collect();
        if self.coinbase {
            return inputs.is_empty();
        }
        if inputs.is_empty() {
            return false;
        }
        verify_balance(&inputs, &outputs, &self.fee_commitment)
    }

    pub fn output_commitments(&self) -> Vec<[u8; 32]> {
        self.real_outputs()
            .into_iter()
            .map(|o| o.value_commitment.commitment)
            .collect()
    }

    pub fn spend_tags(&self) -> Vec<[u8; 32]> {
        self.real_spends()
            .into_iter()
            .map(|s| s.spend_tag)
            .collect()
    }
}

pub fn coinbase_bundle(
    dest: [u8; 32],
    shared_scan: &[u8],
    value_commitment: ValueCommitment,
    asset_id: [u8; 32],
) -> ActionBundle {
    let tag = discovery_tag(shared_scan, 0);
    ActionBundle {
        version: 1,
        actions: vec![CompactAction {
            spend: None,
            output: Some(CompactOutput {
                one_time_dest: dest,
                discovery_tag: tag,
                value_commitment,
                dummy: false,
                asset_id,
            }),
        }],
        fee_commitment: ValueCommitment::identity(),
        coinbase: true,
    }
    .pad_to(BUNDLE_PAD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::commitment::{blinding_from_seed, PedersenGenerators, ValueCommitment};

    #[test]
    fn coinbase_is_padded_and_conserves_shape() {
        let gens = PedersenGenerators::default();
        let r = blinding_from_seed(b"cb");
        let c = ValueCommitment::commit(50, &r, &gens);
        let b = coinbase_bundle([7u8; 32], b"scan", c, [0u8; 32]);
        assert_eq!(b.actions.len(), BUNDLE_PAD);
        assert!(b.verify_conservation());
        assert_eq!(b.real_outputs().len(), 1);
        assert!(b.real_spends().is_empty());
    }
}
