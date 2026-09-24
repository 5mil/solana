//! Listed predicates on notes. Policy is born with the output and spent
//! only against the same-policy slice of the living set.
//! Unknown pred ids fail closed.

use super::keys::SpendPk;
use crate::consensus::pow::sha256d;
use serde::{Deserialize, Serialize};

pub const PRED_PK: [u8; 32] = [0u8; 32];

fn tag(label: &[u8]) -> [u8; 32] {
    sha256d(&[b"orthal-pred-id".as_ref(), label].concat())
}

pub fn id_pk_n() -> [u8; 32] { tag(b"pk-n") }
pub fn id_after() -> [u8; 32] { tag(b"after") }
pub fn id_and() -> [u8; 32] { tag(b"and") }
pub fn id_or() -> [u8; 32] { tag(b"or") }
pub fn id_rate() -> [u8; 32] { tag(b"rate") }
pub fn id_swap() -> [u8; 32] { tag(b"swap") }
pub fn id_pool_lp() -> [u8; 32] { tag(b"pool-lp") }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Predicate {
    Pk,
    PkN { dests: Vec<SpendPk>, n: u8 },
    After { height: u64 },
    And { left: Box<Predicate>, right: Box<Predicate> },
    Or { left: Box<Predicate>, right: Box<Predicate> },
    Rate { dest: SpendPk, max_per_window: u64, window: u64 },
    Swap { want_asset: [u8; 32], pay_asset: [u8; 32], max_in: u64, min_out: u64 },
    PoolLp { pool: [u8; 32], asset_a: [u8; 32], asset_b: [u8; 32] },
}

impl Predicate {
    pub fn id(&self) -> [u8; 32] {
        match self {
            Self::Pk => PRED_PK,
            Self::PkN { .. } => id_pk_n(),
            Self::After { .. } => id_after(),
            Self::And { .. } => id_and(),
            Self::Or { .. } => id_or(),
            Self::Rate { .. } => id_rate(),
            Self::Swap { .. } => id_swap(),
            Self::PoolLp { .. } => id_pool_lp(),
        }
    }
    pub fn commit(&self) -> [u8; 32] {
        sha256d(&bincode::serialize(self).unwrap_or_default())
    }
    pub fn is_default(&self) -> bool { matches!(self, Self::Pk) }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PredWitness {
    pub keys: Vec<u8>,
    pub fill: Vec<u8>,
}

impl PredWitness {
    pub fn none() -> Self { Self::default() }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PredHeader {
    pub id: [u8; 32],
    pub commit: [u8; 32],
}

impl PredHeader {
    pub fn pk() -> Self { Self { id: PRED_PK, commit: [0u8; 32] } }
    pub fn from_pred(p: &Predicate) -> Self {
        if p.is_default() { Self::pk() } else { Self { id: p.id(), commit: p.commit() } }
    }
    pub fn is_default(&self) -> bool { self.id == PRED_PK && self.commit == [0u8; 32] }
}

pub struct PredContext<'a> {
    pub height: u64,
    pub claimed: &'a Predicate,
    pub header: &'a PredHeader,
    pub witness: &'a PredWitness,
    pub out_dests: &'a [SpendPk],
}

pub fn verify_pred(ctx: PredContext<'_>) -> Result<(), &'static str> {
    if ctx.header.id != ctx.claimed.id() { return Err("pred id mismatch"); }
    if !ctx.claimed.is_default() && ctx.header.commit != ctx.claimed.commit() {
        return Err("pred commit mismatch");
    }
    eval(ctx.claimed, &ctx)
}

fn eval(p: &Predicate, ctx: &PredContext<'_>) -> Result<(), &'static str> {
    match p {
        Predicate::Pk => Ok(()),
        Predicate::PkN { dests, n } => {
            if *n as usize == 0 || dests.is_empty() { return Err("empty pk-n"); }
            let mut used = std::collections::BTreeSet::new();
            for i in &ctx.witness.keys {
                let i = *i as usize;
                if i >= dests.len() || !used.insert(i) { return Err("bad pk-n witness"); }
            }
            if used.len() < *n as usize { return Err("pk-n threshold"); }
            Ok(())
        }
        Predicate::After { height } => {
            if ctx.height >= *height { Ok(()) } else { Err("after: too early") }
        }
        Predicate::And { left, right } => { eval(left, ctx)?; eval(right, ctx) }
        Predicate::Or { left, right } => {
            if eval(left, ctx).is_ok() || eval(right, ctx).is_ok() { Ok(()) } else { Err("or: neither") }
        }
        Predicate::Rate { dest, .. } => {
            if ctx.out_dests.iter().any(|d| d == dest) { Ok(()) } else { Err("rate dest missing") }
        }
        Predicate::Swap { .. } => {
            if ctx.witness.fill.is_empty() { Err("swap needs fill") } else { Ok(()) }
        }
        Predicate::PoolLp { .. } => {
            if ctx.witness.fill.is_empty() { Err("lp needs fill") } else { Ok(()) }
        }
    }
}

pub fn known_id(id: &[u8; 32]) -> bool {
    *id == PRED_PK || *id == id_pk_n() || *id == id_after() || *id == id_and()
        || *id == id_or() || *id == id_rate() || *id == id_swap() || *id == id_pool_lp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::keys::SpendKey;
    fn dest(s: &[u8]) -> SpendPk { SpendKey::from_wallet_seed(s).pk() }
    #[test]
    fn after_respects_height() {
        let p = Predicate::After { height: 10 };
        let h = PredHeader::from_pred(&p);
        let w = PredWitness::none();
        let dests: Vec<SpendPk> = vec![];
        let early = PredContext { height: 9, claimed: &p, header: &h, witness: &w, out_dests: &dests };
        assert_eq!(verify_pred(early).unwrap_err(), "after: too early");
        let ok = PredContext { height: 10, claimed: &p, header: &h, witness: &w, out_dests: &dests };
        assert!(verify_pred(ok).is_ok());
    }
    #[test]
    fn pkn_threshold() {
        let dests = vec![dest(b"a"), dest(b"b"), dest(b"c")];
        let p = Predicate::PkN { dests: dests.clone(), n: 2 };
        let h = PredHeader::from_pred(&p);
        let outs: Vec<SpendPk> = vec![];
        let short = PredWitness { keys: vec![0], fill: vec![] };
        let ctx = PredContext { height: 1, claimed: &p, header: &h, witness: &short, out_dests: &outs };
        assert!(verify_pred(ctx).is_err());
        let ok = PredWitness { keys: vec![0, 2], fill: vec![] };
        let ctx = PredContext { height: 1, claimed: &p, header: &h, witness: &ok, out_dests: &outs };
        assert!(verify_pred(ctx).is_ok());
    }
    #[test]
    fn unknown_id_is_not_listed() {
        assert!(!known_id(&[1u8; 32]));
        assert!(known_id(&PRED_PK));
        assert!(known_id(&id_after()));
    }
    #[test]
    fn commit_binds_params() {
        let a = Predicate::After { height: 1 };
        let b = Predicate::After { height: 2 };
        assert_ne!(a.commit(), b.commit());
        assert_eq!(a.id(), b.id());
    }
}
