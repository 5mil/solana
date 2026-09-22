# HybridChain — SHA256d PoW/PoS Blockchain

A classic hybrid Proof-of-Work / Proof-of-Stake cryptocurrency built in Rust.

## Compact notes (`dev`)

Every block includes a padded compact action bundle. Coinbase pays a sealed one-time destination. Header commits to `notes_root` and `tags_root`. Persistence revalidates those roots; a flipped note leaf or root refuses to load.

See [NOTES.md](NOTES.md).

## Build & Run

```bash
cd hybrid-chain
cargo test --all-targets
cargo run -- --data /tmp/hc/chain.bin
cargo run -- --data /tmp/hc/chain.bin --replay
```
