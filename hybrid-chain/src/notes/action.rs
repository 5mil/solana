//! One action type. Dest lives on outputs only.

use super::commitment::{verify_balance, ValueCommitment};
use super::keys::SpendPk;
use super::proof::{BindingSig, EmissionOr, NoteProof};
use serde::{Deserialize, Serialize};

pub const BUNDLE_PAD: usize = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactSpend {
    pub spend_tag: [u8; 32],
    pub rerand: ValueCommitment,
    pub proof: Option<NoteProof>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactOutput {
    pub dest: SpendPk,
    pub eph_pk: [u8; 32],
    pub diversifier: [u8; 16],
    pub value_commitment: ValueCommitment,
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
    pub binding: Option<BindingSig>,
    pub emission: Option<EmissionOr>,
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
                    proof: None,
                }),
                output: Some(CompactOutput {
                    dest: SpendPk { bytes: [0u8; 32] },
                    eph_pk: [0u8; 32],
                    diversifier: [0u8; 16],
                    value_commitment: ValueCommitment::identity(),
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
            .filter(|s| s.spend_tag != [0u8; 32])
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

    pub fn id(&self) -> [u8; 32] {
        crate::consensus::pow::sha256d(&bincode::serialize(self).unwrap_or_default())
    }
}
