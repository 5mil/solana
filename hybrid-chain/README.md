# Orthal (`ORTH`)

Hybrid SHA256d proof-of-work and coin-age proof-of-stake. Compact action
bundles. Native unit **ORTH**. Issued tickers are notes on the same ledger —
not a second virtual machine and not a deployed token program.

This crate is `hybrid-chain` on branch `dev` of
[github.com/5mil/solana](https://github.com/5mil/solana/tree/dev).

```text
mine ORTH  →  birth a ticker  →  buy / sell the curve  →  graduate to an LP note
```

## What you can do with it

| You want | You use |
| --- | --- |
| A mineable coin named ORTH | `mine_pow_block(wallet_seed)` |
| A named ticker that cannot be cloned | `birth_outputs` + `AssetBook`. Symbol is unique forever |
| A fair launch | `LaunchPreset::fair()` — inventory only, no team slice |
| A founder bag that cannot move yet | `LaunchPreset::locked_team()` — extra note under `After` |
| A flatter first hour | `LaunchPreset::deep()` — larger virtual reserve |
| Off-node matching | `Intent` / `IntentBoard` (one leg must be ORTH) |
| Time locks, multisig, swaps, LP | Listed predicates on the note at birth |
| A node that refuses a bad file | `--replay` against the same data directory |

## Native unit

- Name: **Orthal**
- Ticker: **ORTH**
- Asset id: 32 zero bytes
- Cap: 21,000,000
- PoW subsidy: 50 ORTH, halved every 210,000 blocks
- PoW target: 120 s · PoS target: 60 s · retarget window: 2,016
- `ORTH` cannot be issued as a ticker. The book refuses the symbol and the zero id.

Miner payout dest is derived from the wallet seed. A pool ticket is a label,
not a key.

## Issued tickers

A ticker is an inventory note plus a public name. There is no factory contract.

1. Pick a symbol: `A–Z` / `0–9`, length 1–12, not `ORTH`.
2. Asset id is `H("orthal-ticker" ‖ SYMBOL)`. Creator and salt do not mint a
   second copy of the same name.
3. `birth_outputs(...)` creates the inventory note under `Predicate::Curve`
   and, if you asked for a team slice, a second note under `Predicate::After`.
4. The human symbol is written on the output. Replay rebuilds the book from it.
5. A second birth of that symbol fails (`ticker id taken` / `symbol taken`).

Curve quote (virtual constant product):

```text
tokens_out = cap_remaining × quote_in / (virtual + raised + quote_in)
```

Curve state (`quote_raised`, `cap_remaining`) lives on the inventory note.
When `quote_raised ≥ graduate_quote`, that note may be spent into a
`pool-lp` note. Buys after that use swap / LP fills, not the birth curve.

Helpers in `src/notes/market.rs`:

- `LaunchPreset::{fair, locked_team, deep}`
- `preview_buy` / `preview_sell`
- `IntentBoard` — rejects posts where neither leg is ORTH
- `path_for_user(...)` — which path a wallet should offer

What this path does not do: tax tokens, rebases, reflections, or a public
holder leaderboard. Those need a roster of accounts. Amounts stay inside
the value commitment.

## Programs (predicates, not a VM)

Policy is born with the output. A spend is checked against that policy and
against the same-policy slice of the living set. Unknown predicate ids fail
closed. There is no general-purpose bytecode on the node.

| Id | Predicate | What it enforces |
| --- | --- | --- |
| default | `Pk` | Spend key |
| `pk-n` | `PkN` | N of M dests |
| `after` | `After` | Not spendable before height |
| `and` / `or` | `And` / `Or` | Combine listed preds |
| `rate` | `Rate` | Drip to a dest each window |
| `swap` | `Swap` | Fill must name pay / want assets |
| `pool-lp` | `PoolLp` | LP note for a pair |
| `curve` | `Curve` | Birth inventory + virtual product |
| `ticker` | `Ticker` | Named asset; symbol required |

Intents ride on the same bundle (`Intent`, `IntentFill`). An unknown
`ExecProof` program id fails closed. Batch settlement is listed fill plus
conservation, not an on-node matching engine.

`And` / `Or` compose only listed children. That is the whole program surface
for launch.

## Chain structure

```text
hybrid-chain/
  src/
    main.rs            node: mine or --replay
    params.rs          Orthal / ORTH / subsidy / windows
    consensus/         SHA256d PoW, coin-age PoS, retarget
    chain/
      blockchain.rs    apply bundles, mine, register tickers
      block.rs         header: merkle, notes_root, tags_root
      store.rs         snapshot + revalidate + rebuild
      pool.rs          default share path
    notes/
      action.rs        CompactOutput / ActionBundle (one codec)
      asset.rs         ORTH, ticker_id, AssetBook
      ticker.rs        CurveSpec, birth_outputs
      market.rs        presets, quotes, board, user paths
      pred.rs          listed predicates
      intent.rs        want / pay / expire / fill
      proof.rs         membership + binding + emission OR
      spend.rs         transfer + emission constructors
      launch.rs        living window
      payout.rs        dest from wallet seed
      keys.rs          spend / scan
      stake.rs         PoS proof against a commitment
    wallet/            local seed → dest
```

Every block is compact-only. Header commits `notes_root` and `tags_root`.
Conservation is a binding signature on the residual of the commitments
(`r ≠ 0`). Membership is against the living window.

## Consensus

- **PoW** — SHA256d, header nonce, difficulty bits.
- **PoS** — coin-age against a committed stake; `StakeProof` is required.
  Reward is not a naked integer the miner types in.
- **Emission** — coinbase bundle with an emission proof. Issuance of a
  ticker is a different bundle kind (`is_issuance`): no coinbase proof,
  non-empty symbol / asset on the outputs.
- **Pool** — `DefaultPool` accepts a share whose dest matches the seed
  that mined it.

## Persistence

```bash
cargo run -- --data /tmp/orthal/chain.bin
cargo run -- --data /tmp/orthal/chain.bin --replay
```

Replay rebuilds the living set, the tag set, and the ticker book from the
blocks. A flipped nonce, parent, note leaf, or root refuses to load. Use a
new directory for experiments. Do not edit `chain.bin`.

## Build

```bash
git clone https://github.com/5mil/solana.git
cd solana && git checkout dev && cd hybrid-chain
cargo test --all-targets
cargo run -- --data /tmp/orthal/chain.bin
```

Expected on a fresh directory: banner `Orthal` / ticker `ORTH`, a mined
height, a dest public key, `pool accepted`, a written snapshot. Replay
prints the same height.

Before you ship a binary:

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

## Worked calls

Birth (locked team):

```rust
let preset = LaunchPreset::locked_team();
let (bundle, spec, asset) = birth_outputs(
    &creator, "MEME", b"ignored-for-id",
    preset.cap, preset.virtual_quote, preset.graduate_quote,
    start, preset.team_value, start + preset.team_unlock_delta, height,
)?;
chain.apply_transfer(&bundle)?;
assert!(chain.assets.symbol_taken("MEME"));
```

A second `birth_outputs(..., "MEME", ...)` from any wallet produces the
same asset id and is rejected by the book.

Quote before you sign:

```rust
let q = preview_buy(&spec, quote_in)?;
// q.tokens_out, q.graduates
```

## Status

`dev` is the integration branch. Predicate ids are listed and fail closed.
Range proofs are still a 48-bit sigma construction — not a production
Bulletproofs circuit. Relay is a first-hop stem, not a full network.
Unknown `ExecProof` programs do not run.

Further reading: [NOTES.md](NOTES.md).
