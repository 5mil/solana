//! One action type. Padding is the identity point, not a trusted flag.

use super::auth::{BindingSig, LinkProof, RangeProof};
use super::commitment::{blinding_from_seed, verify_balance, PedersenGenerators, ValueCommitment};
use super::payout::discovery_tag;
use curve25519_dalek::scalar::Scalar;
use serde::{Deserialize, Serialize};

pub const BUNDLE_PAD: usize = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactSpend {
    pub spend_tag: [u8; 32],
    pub rerand: ValueCommitment,
    pub ring: Vec<[u8; 32]>,
    pub link: Option<LinkProof>,
    pub range: Option<RangeProof>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactOutput {
    pub one_time_dest: [u8; 32],
    pub discovery_tag: [u8; 32],
    pub value_commitment: ValueCommitment,
    pub asset_id: [u8; 32],
    pub range: Option<RangeProof>,
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
    pub fee_range: Option<RangeProof>,
    pub binding: Option<BindingSig>,
}

fn is_identity(c: &ValueCommitment) -> bool {
    c.commitment == ValueCommitment::identity().commitment
}

impl ActionBundle {
    pub fn pad_to(mut self, n: usize) -> Self {
        while self.actions.len() < n {
            self.actions.push(CompactAction {
                spend: Some(CompactSpend {
                    spend_tag: [0u8; 32],
                    rerand: ValueCommitment::identity(),
                    ring: Vec::new(),
                    link: None,
                    range: None,
                }),
                output: Some(CompactOutput {
                    one_time_dest: [0u8; 32],
                    discovery_tag: [0u8; 32],
                    value_commitment: ValueCommitment::identity(),
                    asset_id: [0u8; 32],
                    range: None,
                }),
            });
        }
        self
    }

    pub fn real_outputs(&self) -> Vec<&CompactOutput> {
        self.actions
            .iter()
            .filter_map(|a| a.output.as_ref())
            .filter(|o| !is_identity(&o.value_commitment))
            .collect()
    }

    pub fn real_spends(&self) -> Vec<&CompactSpend> {
        self.actions
            .iter()
            .filter_map(|a| a.spend.as_ref())
            .filter(|s| s.spend_tag != [0u8; 32] && !is_identity(&s.rerand))
            .collect()
    }

    pub fn is_emission(&self) -> bool {
        self.real_spends().is_empty() && !self.real_outputs().is_empty()
    }

    pub fn verify_conservation(&self) -> bool {
        let inputs: Vec<ValueCommitment> = self
            .real_spends()
            .into_iter()
            .map(|s| s.rerand.clone())
            .collect();
        let outputs: Vec<ValueCommitment> = self
            .real_outputs()
            .into_iter()
            .map(|o| o.value_commitment.clone())
            .collect();
        if inputs.is_empty() {
            return !outputs.is_empty() && is_identity(&self.fee_commitment);
        }
        if !verify_balance(&inputs, &outputs, &self.fee_commitment) {
            return false;
        }
        if let Some(sig) = &self.binding {
            let mut t = Vec::new();
            for s in self.real_spends() {
                t.extend_from_slice(&s.spend_tag);
            }
            for o in self.real_outputs() {
                t.extend_from_slice(&o.value_commitment.commitment);
            }
            return sig.verify(&inputs, &outputs, &self.fee_commitment, &t);
        }
        false
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

pub fn emission_blinding(dest: &[u8; 32], height: u64) -> Scalar {
    let mut seed = b"hybrid-emission-v2".to_vec();
    seed.extend_from_slice(dest);
    seed.extend_from_slice(&height.to_le_bytes());
    blinding_from_seed(&seed)
}

pub fn emission_commitment(dest: &[u8; 32], height: u64, reward: u64) -> ValueCommitment {
    ValueCommitment::commit(
        reward,
        &emission_blinding(dest, height),
        &PedersenGenerators::default(),
    )
}

pub fn coinbase_bundle(
    dest: [u8; 32],
    shared_scan: &[u8],
    value_commitment: ValueCommitment,
    asset_id: [u8; 32],
    range: RangeProof,
) -> ActionBundle {
    let tag = discovery_tag(shared_scan, 0);
    ActionBundle {
        version: 2,
        actions: vec![CompactAction {
            spend: None,
            output: Some(CompactOutput {
                one_time_dest: dest,
                discovery_tag: tag,
                value_commitment,
                asset_id,
                range: Some(range),
            }),
        }],
        fee_commitment: ValueCommitment::identity(),
        fee_range: None,
        binding: None,
    }
    .pad_to(BUNDLE_PAD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::auth::RangeProof;

    #[test]
    fn emission_shape_is_padded() {
        let dest = [7u8; 32];
        let r = emission_blinding(&dest, 0);
        let c = emission_commitment(&dest, 0, 50);
        let range = RangeProof::prove(50, &r);
        let b = coinbase_bundle(dest, b"scan", c, [0u8; 32], range);
        assert_eq!(b.actions.len(), BUNDLE_PAD);
        assert!(b.is_emission());
        assert!(b.verify_conservation());
    }
}
