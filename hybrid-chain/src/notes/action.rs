//! One action type. Dest lives on outputs only.
//! Predicates and intents ride on the same bundle.

use super::commitment::ValueCommitment;
use super::intent::{verify_exec, verify_intents, ExecProof, Intent, IntentFill};
use super::keys::SpendPk;
use super::pred::{known_id, PredHeader, PredWitness, Predicate, PRED_PK};
use super::proof::{BindingSig, EmissionOr, NoteProof};
use serde::{Deserialize, Serialize};

pub const BUNDLE_PAD: usize = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactSpend {
    pub spend_tag: [u8; 32],
    pub rerand: ValueCommitment,
    pub proof: Option<NoteProof>,
    #[serde(default)]
    pub pred: PredHeader,
    #[serde(default)]
    pub pred_body: Option<Predicate>,
    #[serde(default)]
    pub pred_witness: PredWitness,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompactOutput {
    pub dest: SpendPk,
    pub eph_pk: [u8; 32],
    pub diversifier: [u8; 16],
    pub value_commitment: ValueCommitment,
    #[serde(default)]
    pub pred: PredHeader,
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
    #[serde(default)]
    pub intents: Vec<Intent>,
    #[serde(default)]
    pub fills: Vec<IntentFill>,
    #[serde(default)]
    pub exec: Option<ExecProof>,
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
                    pred: PredHeader::pk(),
                    pred_body: None,
                    pred_witness: PredWitness::none(),
                }),
                output: Some(CompactOutput {
                    dest: SpendPk { bytes: [0u8; 32] },
                    eph_pk: [0u8; 32],
                    diversifier: [0u8; 16],
                    value_commitment: ValueCommitment::identity(),
                    pred: PredHeader::pk(),
                }),
            });
        }
        self
    }

    pub fn real_outputs(&self) -> Vec<&CompactOutput> {
        self.actions.iter().filter_map(|a| a.output.as_ref()).filter(|o| !is_identity(&o.value_commitment)).collect()
    }

    pub fn real_spends(&self) -> Vec<&CompactSpend> {
        self.actions.iter().filter_map(|a| a.spend.as_ref()).filter(|s| s.spend_tag != [0u8; 32]).collect()
    }

    pub fn is_emission(&self) -> bool {
        self.real_spends().is_empty() && !self.real_outputs().is_empty()
    }

    pub fn verify_conservation(&self) -> bool {
        let inputs: Vec<ValueCommitment> = self.real_spends().into_iter().map(|s| s.rerand.clone()).collect();
        let outputs: Vec<ValueCommitment> = self.real_outputs().into_iter().map(|o| o.value_commitment.clone()).collect();
        if inputs.is_empty() {
            return !outputs.is_empty() && is_identity(&self.fee_commitment);
        }
        self.binding.is_some() && !outputs.is_empty()
    }

    pub fn verify_programs(&self, height: u64) -> Result<(), &'static str> {
        verify_exec(&self.exec)?;
        verify_intents(&self.intents, &self.fills, self.actions.len(), height)?;
        let dests: Vec<SpendPk> = self.real_outputs().into_iter().map(|o| o.dest.clone()).collect();
        for o in self.real_outputs() {
            if !known_id(&o.pred.id) { return Err("unknown output pred"); }
        }
        for s in self.real_spends() {
            if !known_id(&s.pred.id) { return Err("unknown spend pred"); }
            if s.pred.id != PRED_PK {
                let body = s.pred_body.as_ref().ok_or("pred body required")?;
                super::pred::verify_pred(super::pred::PredContext {
                    height, claimed: body, header: &s.pred, witness: &s.pred_witness, out_dests: &dests,
                })?;
            }
        }
        Ok(())
    }

    pub fn output_commitments(&self) -> Vec<[u8; 32]> {
        self.real_outputs().into_iter().map(|o| o.value_commitment.commitment).collect()
    }

    pub fn output_records(&self) -> Vec<super::launch::LiveNote> {
        self.real_outputs().into_iter().map(|o| super::launch::LiveNote {
            cm: o.value_commitment.commitment, pred: o.pred.id, pred_commit: o.pred.commit,
        }).collect()
    }

    pub fn spend_tags(&self) -> Vec<[u8; 32]> {
        self.real_spends().into_iter().map(|s| s.spend_tag).collect()
    }

    pub fn id(&self) -> [u8; 32] {
        crate::consensus::pow::sha256d(&bincode::serialize(self).unwrap_or_default())
    }
}
