# HybridChain — SHA256d PoW/PoS Blockchain

A classic hybrid Proof-of-Work / Proof-of-Stake cryptocurrency built in Rust.

## Architecture

```
hybrid-chain/
├── src/
│   ├── main.rs                    # Node entry point
│   ├── params.rs                  # Chain parameters (coin name, supply, rewards)
│   ├── consensus/
│   │   ├── pow.rs                 # SHA256d mining, difficulty check
│   │   ├── pos.rs                 # Coin-age staking, stake validation
│   │   └── difficulty.rs         # Retargeting algorithm
│   ├── chain/
│   │   ├── block.rs               # Block + BlockHeader structures
│   │   ├── blockchain.rs          # Chain state, mine/mint functions
│   │   └── transaction.rs        # Tx, coinbase, coinstake
│   └── wallet/
│       └── keys.rs                # Address derivation (stub)
└── Cargo.toml
```

## Consensus Model

| Parameter | Value |
|-----------|-------|
| PoW Algorithm | SHA256d (double SHA256) |
| PoS Model | Coin-age (Peercoin-style) |
| PoW Block Time | 2 minutes |
| PoS Block Time | 1 minute |
| PoW Reward | 50 coins (halving every 210,000 blocks) |
| PoS Rate | 5% annual |
| Min Stake | 100 coins |
| Min Coin Age | 1 day |
| Max Coin Age | 90 days |
| Max Supply | 21,000,000 coins |
| Difficulty Window | 2,016 blocks |

## Build & Run

```bash
cd hybrid-chain
cargo build --release
cargo run
cargo test
```

## Next Steps

- [ ] secp256k1 keypair generation + ECDSA signing
- [ ] P2P networking layer (libp2p)
- [ ] UTXO set / mempool
- [ ] RPC server (JSON-RPC)
- [ ] Wallet CLI
- [ ] Stake kernel hash (PoS security hardening)
- [ ] Checkpoint system
- [ ] Explorer frontend
