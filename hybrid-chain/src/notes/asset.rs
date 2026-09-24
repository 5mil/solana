//! Native ORTH and first-seen ticker ids.
//! Asset id is public. Amount stays in the commitment.

use crate::consensus::pow::sha256d;
use super::keys::SpendPk;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ORTH: [u8; 32] = [0u8; 32];

pub fn ticker_id(symbol: &str, creator: &SpendPk, salt: &[u8]) -> [u8; 32] {
    let sym = normalize_symbol(symbol).unwrap_or("");
    sha256d(&[b"orthal-ticker".as_ref(), sym.as_bytes(), &creator.bytes, salt].concat())
}

pub fn normalize_symbol(symbol: &str) -> Result<&str, &'static str> {
    let s = symbol.trim();
    if s.is_empty() || s.len() > 12 { return Err("symbol 1..=12"); }
    if !s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
        return Err("symbol A-Z0-9");
    }
    if s == "ORTH" { return Err("ORTH is native"); }
    Ok(s)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TickerRecord {
    pub asset: [u8; 32],
    pub symbol: String,
    pub creator: SpendPk,
    pub height: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AssetBook {
    by_id: BTreeMap<[u8; 32], TickerRecord>,
    by_symbol: BTreeMap<String, [u8; 32]>,
}

impl AssetBook {
    pub fn new() -> Self { Self::default() }
    pub fn get(&self, asset: &[u8; 32]) -> Option<&TickerRecord> {
        if *asset == ORTH { return None; }
        self.by_id.get(asset)
    }
    pub fn is_orth(asset: &[u8; 32]) -> bool { *asset == ORTH }
    pub fn register(&mut self, rec: TickerRecord) -> Result<(), &'static str> {
        if rec.asset == ORTH { return Err("ORTH is not issued"); }
        if self.by_id.contains_key(&rec.asset) { return Err("ticker id taken"); }
        if self.by_symbol.contains_key(&rec.symbol) { return Err("symbol taken"); }
        self.by_symbol.insert(rec.symbol.clone(), rec.asset);
        self.by_id.insert(rec.asset, rec);
        Ok(())
    }
    pub fn len(&self) -> usize { self.by_id.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::keys::SpendKey;
    #[test]
    fn orth_reserved() {
        assert!(AssetBook::is_orth(&ORTH));
        let pk = SpendKey::from_wallet_seed(b"c").pk();
        assert!(normalize_symbol("ORTH").is_err());
        assert_ne!(ticker_id("DOGE", &pk, b"1"), ticker_id("DOGE", &pk, b"2"));
    }
    #[test]
    fn first_seen_wins() {
        let mut book = AssetBook::new();
        let pk = SpendKey::from_wallet_seed(b"c").pk();
        let id = ticker_id("MEME", &pk, b"s");
        let rec = TickerRecord { asset: id, symbol: "MEME".into(), creator: pk.clone(), height: 1 };
        assert!(book.register(rec.clone()).is_ok());
        assert!(book.register(rec).is_err());
        let other = ticker_id("MEME", &SpendKey::from_wallet_seed(b"x").pk(), b"s");
        let rec2 = TickerRecord { asset: other, symbol: "MEME".into(), creator: pk, height: 2 };
        assert!(book.register(rec2).is_err());
    }
}
