//! Native ORTH and a first-seen symbol book.
//! A ticker name is taken forever. Asset id is the hash of the symbol.

use crate::consensus::pow::sha256d;
use super::keys::SpendPk;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ORTH: [u8; 32] = [0u8; 32];

/// Asset id is only the symbol. Salt and creator cannot mint a second MEME.
pub fn ticker_id(symbol: &str) -> Result<[u8; 32], &'static str> {
    let sym = normalize_symbol(symbol)?;
    Ok(sha256d(&[b"orthal-ticker".as_ref(), sym.as_bytes()].concat()))
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
    pub fn get(&self, asset: &[u8; 32]) -> Option<&TickerRecord> { self.by_id.get(asset) }
    pub fn by_symbol(&self, symbol: &str) -> Option<&TickerRecord> {
        let id = self.by_symbol.get(symbol)?;
        self.by_id.get(id)
    }
    pub fn is_orth(asset: &[u8; 32]) -> bool { *asset == ORTH }
    pub fn symbol_taken(&self, symbol: &str) -> bool {
        symbol == "ORTH" || self.by_symbol.contains_key(symbol)
    }
    pub fn register(&mut self, rec: TickerRecord) -> Result<(), &'static str> {
        let sym = normalize_symbol(&rec.symbol)?.to_string();
        if rec.asset == ORTH { return Err("ORTH is not issued"); }
        let expect = ticker_id(&sym)?;
        if rec.asset != expect { return Err("asset id must be hash(symbol)"); }
        if self.by_id.contains_key(&rec.asset) { return Err("ticker id taken"); }
        if self.by_symbol.contains_key(&sym) { return Err("symbol taken"); }
        let mut rec = rec;
        rec.symbol = sym.clone();
        self.by_symbol.insert(sym, rec.asset);
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
        assert!(normalize_symbol("ORTH").is_err());
        assert!(ticker_id("ORTH").is_err());
    }
    #[test]
    fn same_symbol_same_id() {
        assert_eq!(ticker_id("MEME").unwrap(), ticker_id("MEME").unwrap());
        assert_ne!(ticker_id("MEME").unwrap(), ticker_id("PEPE").unwrap());
    }
    #[test]
    fn symbol_cannot_be_reissued() {
        let mut book = AssetBook::new();
        let pk = SpendKey::from_wallet_seed(b"c").pk();
        let id = ticker_id("MEME").unwrap();
        let rec = TickerRecord { asset: id, symbol: "MEME".into(), creator: pk.clone(), height: 1 };
        assert!(book.register(rec.clone()).is_ok());
        assert!(book.symbol_taken("MEME"));
        assert_eq!(book.register(rec).unwrap_err(), "ticker id taken");
    }
    #[test]
    fn forged_id_rejected() {
        let mut book = AssetBook::new();
        let pk = SpendKey::from_wallet_seed(b"c").pk();
        let rec = TickerRecord { asset: [9u8; 32], symbol: "MEME".into(), creator: pk, height: 1 };
        assert_eq!(book.register(rec).unwrap_err(), "asset id must be hash(symbol)");
    }
}
