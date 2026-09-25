//! ORTH is the native ticker. Issued tickers are notes with a public asset id.

use super::action::{ActionBundle, CompactAction, CompactOutput};
use super::asset::{normalize_symbol, ticker_id, ORTH};
use super::commitment::{blinding_from_seed, ValueCommitment};
use super::intent::{Intent, IntentFill};
use super::keys::{SpendKey, SpendPk};
use super::pred::{PredHeader, Predicate};
use super::proof::commit_with_asset;
use curve25519_dalek::constants::RISTRETTO_BASEPOINT_POINT;
use curve25519_dalek::scalar::Scalar;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CurveSpec {
    pub asset: [u8; 32],
    pub cap: u64,
    pub virtual_quote: u64,
    pub graduate_quote: u64,
    pub start_height: u64,
    pub quote_raised: u64,
    pub cap_remaining: u64,
}

impl CurveSpec {
    pub fn new(asset: [u8; 32], cap: u64, virtual_quote: u64, graduate_quote: u64, start: u64) -> Self {
        Self { asset, cap, virtual_quote, graduate_quote, start_height: start, quote_raised: 0, cap_remaining: cap }
    }
    pub fn as_pred(&self) -> Predicate {
        Predicate::Curve {
            asset: self.asset, cap: self.cap, virtual_quote: self.virtual_quote,
            graduate_quote: self.graduate_quote, start_height: self.start_height,
            quote_raised: self.quote_raised, cap_remaining: self.cap_remaining,
        }
    }
    pub fn tokens_out(&self, quote_in: u64) -> Result<u64, &'static str> {
        if quote_in == 0 { return Err("zero buy"); }
        let den = self.virtual_quote.checked_add(self.quote_raised).and_then(|x| x.checked_add(quote_in)).ok_or("curve den")?;
        let num = (self.cap_remaining as u128).checked_mul(quote_in as u128).ok_or("curve num")?;
        let out = (num / den as u128) as u64;
        if out == 0 || out > self.cap_remaining { return Err("curve empty"); }
        Ok(out)
    }
    pub fn after_buy(&self, quote_in: u64) -> Result<(Self, u64), &'static str> {
        let got = self.tokens_out(quote_in)?;
        let mut n = self.clone();
        n.quote_raised = n.quote_raised.saturating_add(quote_in);
        n.cap_remaining = n.cap_remaining.saturating_sub(got);
        Ok((n, got))
    }
    pub fn graduated(&self) -> bool { self.quote_raised >= self.graduate_quote }
}

fn eph(label: &[u8], div: &[u8; 16], pk: &[u8; 32]) -> [u8; 32] {
    let sk = blinding_from_seed(&[label, div.as_slice(), pk].concat());
    (sk * RISTRETTO_BASEPOINT_POINT).compress().to_bytes()
}

fn note(dest: SpendPk, value: u64, blind: &Scalar, asset: [u8; 32], symbol: String, pred: PredHeader, div: [u8; 16]) -> CompactOutput {
    CompactOutput {
        dest: dest.clone(),
        eph_pk: eph(b"eph-t", &div, &dest.bytes),
        diversifier: div,
        value_commitment: commit_with_asset(value, blind, &ORTH),
        pred,
        asset,
        symbol,
    }
}

pub fn birth_outputs(
    creator: &SpendKey, symbol: &str, salt: &[u8], cap: u64, virtual_quote: u64,
    graduate_quote: u64, start_height: u64, team_value: u64, team_unlock: u64, height: u64,
) -> Result<(ActionBundle, CurveSpec, [u8; 32]), &'static str> {
    let _ = height;
    let _ = salt;
    let sym = normalize_symbol(symbol)?.to_string();
    let asset = ticker_id(&sym)?;
    if cap == 0 || virtual_quote == 0 { return Err("cap/virtual"); }
    let spec = CurveSpec::new(asset, cap, virtual_quote, graduate_quote, start_height);
    let inv_r = blinding_from_seed(&[b"inv-r".as_ref(), &asset, &cap.to_le_bytes()].concat());
    let inv = note(creator.pk(), cap, &inv_r, asset, sym.clone(), PredHeader::from_pred(&spec.as_pred()), [1u8; 16]);
    let mut actions = vec![CompactAction { spend: None, output: Some(inv) }];
    if team_value > 0 {
        if team_value >= cap { return Err("team >= cap"); }
        let team_pred = Predicate::After { height: team_unlock };
        let team_r = blinding_from_seed(&[b"team-r".as_ref(), &asset].concat());
        actions.push(CompactAction { spend: None, output: Some(note(
            creator.pk(), team_value, &team_r, asset, sym.clone(), PredHeader::from_pred(&team_pred), [2u8; 16],
        )) });
    }
    let intent = Intent { id: asset, want_asset: asset, pay_asset: ORTH, expire_height: u64::MAX, bound: spec.as_pred().commit() };
    Ok((ActionBundle {
        version: 7, actions, fee_commitment: ValueCommitment::identity(),
        binding: None, emission: None,
        intents: vec![intent], fills: vec![IntentFill { intent_id: asset, action_index: 0 }], exec: None,
    }, spec, asset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::asset::normalize_symbol;
    use crate::notes::keys::SpendKey;
    #[test]
    fn curve_is_conservative() {
        let spec = CurveSpec::new([7u8; 32], 1_000_000, 10_000, 50_000, 0);
        let a = spec.tokens_out(1_000).unwrap();
        assert!(spec.tokens_out(2_000).unwrap() > a);
        let (n, got) = spec.after_buy(1_000).unwrap();
        assert_eq!(got, a);
        assert_eq!(n.quote_raised, 1_000);
    }
    #[test]
    fn birth_reserves_orth_symbol() {
        assert!(normalize_symbol("ORTH").is_err());
        let sk = SpendKey::from_wallet_seed(b"founder");
        let (b, spec, id) = birth_outputs(&sk, "MEME", b"salt", 1_000_000, 10_000, 80_000, 1, 0, 0, 1).unwrap();
        assert_eq!(spec.asset, id);
        assert_eq!(b.real_outputs()[0].asset, id);
        assert_eq!(b.real_outputs()[0].symbol, "MEME");
        assert_ne!(id, ORTH);
    }
    #[test]
    fn same_name_same_asset() {
        let a = SpendKey::from_wallet_seed(b"a");
        let b = SpendKey::from_wallet_seed(b"b");
        let (_, _, id_a) = birth_outputs(&a, "MEME", b"s1", 1000, 10, 50, 1, 0, 0, 1).unwrap();
        let (_, _, id_b) = birth_outputs(&b, "MEME", b"s2", 1000, 10, 50, 1, 0, 0, 1).unwrap();
        assert_eq!(id_a, id_b);
    }
}
