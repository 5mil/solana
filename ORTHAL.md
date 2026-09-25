# Orthal

**Branch:** [`dev`](https://github.com/5mil/solana/tree/dev)
**Crate:** [`hybrid-chain`](hybrid-chain/)
**Ticker:** ORTH

Orthal is a hybrid SHA256d PoW / coin-age PoS chain. Transfers, miner
payouts, issued tickers, and predicate programs share one compact action
bundle. There is no token-factory VM.

Start here: **[hybrid-chain/README.md](hybrid-chain/README.md)**

```text
mine ORTH → birth a unique ticker → trade the curve → graduate to an LP note
```

- Symbols are unique forever. Asset id is `H(symbol)`. `ORTH` is reserved.
- Programs are listed predicates (`Pk`, `After`, `PkN`, `Curve`, `Swap`, …).
  Unknown ids fail closed.
- Persistence: `--data DIR` then `--replay`. A tampered file does not load.
