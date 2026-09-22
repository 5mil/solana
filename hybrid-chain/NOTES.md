# Compact notes on hybrid-chain (`dev`)

Foundation for Phases 1–3 of the privacy plan. Transparent txs remain for reward accounting; every block also carries a padded compact bundle.

## In this commit

- Ristretto Pedersen commitments and homomorphic balance
- Dummy-padded `ActionBundle` (pad=2)
- Coinbase notes to a **sealed** one-time dest (not the worker label)
- Append-only note Merkle tree; `header.notes_root`
- Spend-tag set; `header.tags_root`
- Load path rebuilds tree+tags and refuses mismatches
- Discovery tags for wallet scan

## Not in this commit

- Halo 2 / zk-token-sdk range proofs wired in
- Global membership proofs (tree inclusion helper only)
- Magister WabiSabi payout coordinator
- Cashu mint
- Agave monorepo merge
- Private PoS (stake still uses transparent coinstake + compact emission note)

## Header fields added

`notes_root`, `tags_root` — old `chain.bin` files will not load. Fresh mine required.

## Tests

```
cd hybrid-chain
cargo test --all-targets
```
