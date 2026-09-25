//! Advanced ticker market: presets, quote preview, graduate, intent board.

use super::asset::ORTH;
use super::ticker::CurveSpec;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchKind { Fair, LockedTeam, Deep }

#[derive(Clone, Debug)]
pub struct LaunchPreset {
    pub kind: LaunchKind,
    pub cap: u64,
    pub virtual_quote: u64,
    pub graduate_quote: u64,
    pub team_value: u64,
    pub team_unlock_delta: u64,
}

impl LaunchPreset {
    pub fn fair() -> Self {
        Self { kind: LaunchKind::Fair, cap: 1_000_000_0000, virtual_quote: 50_0000_0000, graduate_quote: 200_0000_0000, team_value: 0, team_unlock_delta: 0 }
    }
    pub fn locked_team() -> Self {
        Self { kind: LaunchKind::LockedTeam, cap: 1_000_000_0000, virtual_quote: 50_0000_0000, graduate_quote: 200_0000_0000, team_value: 50_000_0000, team_unlock_delta: 10_000 }
    }
    pub fn deep() -> Self {
        Self { kind: LaunchKind::Deep, cap: 2_000_000_0000, virtual_quote: 200_0000_0000, graduate_quote: 800_0000_0000, team_value: 80_000_0000, team_unlock_delta: 20_000 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Quote {
    pub quote_in: u64, pub tokens_out: u64, pub quote_raised_after: u64,
    pub cap_remaining_after: u64, pub graduates: bool,
}

pub fn preview_buy(spec: &CurveSpec, quote_in: u64) -> Result<Quote, &'static str> {
    let (next, tokens) = spec.after_buy(quote_in)?;
    Ok(Quote { quote_in, tokens_out: tokens, quote_raised_after: next.quote_raised, cap_remaining_after: next.cap_remaining, graduates: next.graduated() })
}

pub fn preview_sell(spec: &CurveSpec, tokens_in: u64) -> Result<u64, &'static str> {
    let sold = spec.cap.saturating_sub(spec.cap_remaining);
    if tokens_in == 0 || tokens_in > sold { return Err("sell exceeds sold"); }
    let q = spec.virtual_quote.saturating_add(spec.quote_raised);
    let den = spec.cap_remaining.saturating_add(tokens_in);
    if den == 0 { return Err("sell den"); }
    let out = (q as u128 * tokens_in as u128 / den as u128) as u64;
    if out == 0 || out > spec.quote_raised { return Err("sell empty"); }
    Ok(out)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoardIntent { pub id: [u8; 32], pub want: [u8; 32], pub pay: [u8; 32], pub expire_height: u64 }

#[derive(Clone, Debug, Default)]
pub struct IntentBoard { pub open: Vec<BoardIntent> }

impl IntentBoard {
    pub fn post(&mut self, it: BoardIntent) -> Result<(), &'static str> {
        if it.want == it.pay { return Err("want == pay"); }
        if it.pay != ORTH && it.want != ORTH { return Err("one leg must be ORTH"); }
        if self.open.iter().any(|x| x.id == it.id) { return Err("dup intent"); }
        self.open.push(it); Ok(())
    }
    pub fn take(&mut self, id: [u8; 32]) -> Option<BoardIntent> {
        let i = self.open.iter().position(|x| x.id == id)?;
        Some(self.open.remove(i))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarketPath { MineOrth, BirthTicker, BuyOnCurve, SellOnCurve, GraduateLp, TransferNote, TeamUnlock }

impl MarketPath {
    pub fn label(self) -> &'static str {
        match self {
            Self::MineOrth => "mine ORTH",
            Self::BirthTicker => "birth ticker",
            Self::BuyOnCurve => "buy on curve",
            Self::SellOnCurve => "sell on curve",
            Self::GraduateLp => "graduate to LP note",
            Self::TransferNote => "transfer compact note",
            Self::TeamUnlock => "spend After team note",
        }
    }
}

pub fn path_for_user(has_orth: bool, has_ticker: bool, spec: Option<&CurveSpec>, height: u64, team_unlock: u64) -> Vec<MarketPath> {
    if !has_orth { return vec![MarketPath::MineOrth]; }
    if !has_ticker { return vec![MarketPath::BirthTicker, MarketPath::BuyOnCurve]; }
    let mut p = vec![MarketPath::BuyOnCurve, MarketPath::SellOnCurve, MarketPath::TransferNote];
    if spec.map(|s| s.graduated()).unwrap_or(false) { p.push(MarketPath::GraduateLp); }
    if height >= team_unlock && team_unlock > 0 { p.push(MarketPath::TeamUnlock); }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buy_then_sell_roundtrip_direction() {
        let spec = CurveSpec::new([7u8; 32], 1_000_000, 10_000, 80_000, 0);
        let q = preview_buy(&spec, 1_000).unwrap();
        let (next, _) = spec.after_buy(1_000).unwrap();
        let back = preview_sell(&next, q.tokens_out).unwrap();
        assert!(back > 0 && back <= 1_000);
    }
    #[test]
    fn board_requires_orth_leg() {
        let mut b = IntentBoard::default();
        assert!(b.post(BoardIntent { id: [1u8; 32], want: [2u8; 32], pay: [3u8; 32], expire_height: 9 }).is_err());
        assert!(b.post(BoardIntent { id: [1u8; 32], want: [2u8; 32], pay: ORTH, expire_height: 9 }).is_ok());
    }
    #[test]
    fn new_user_is_told_to_mine() {
        assert_eq!(path_for_user(false, false, None, 0, 0), vec![MarketPath::MineOrth]);
    }
}
