# Compact notes on hybrid-chain (`dev`)

Production-path increment. Reward amounts remain on the classic coinbase for schedule checks; dest is sealed bytes, not a worker label.

## On this branch

- Ristretto Pedersen + homomorphic conservation
- Dummy-padded ActionBundle (pad=2)
- Coinbase / coinstake dest = sealed one-time bytes
- Note Merkle tree + header.notes_root
- Spend-tag set + header.tags_root
- Real spend with membership path (not a decoy list)
- Duplicate spend tag refused
- Compact scan index by discovery tag
- Stem/fluff first-hop model (lab)
- Stake proof binds a committed note
- Pool stores ticket_hash, not dest
- Persist rebuilds tree+tags and refuses flipped leaves / roots / bad membership
- Bounded-epoch forest (`epoch.rs`)
- Launch set on the node (`Blockchain.launch`)

## Five-step cutover (this increment)

1. `Blockchain.launch: LaunchSet` — every compact output is appended to the window.
2. `verify_bundle_against` accepts `HiddenProof` against `window_root`.
3. `rebuild_notes` + persist load replay the launch set; commitment must match.
4. Spends whose leaf is not in the window are refused (`spend outside window`).
5. Tests: hidden spend + mine, replay refused, persist after hidden spend, out-of-window prove is `None`.

`header.notes_root` is still the live-tree root so existing snapshots keep their formula.

## Still not production-complete

- Halo 2 / zk-token-sdk range circuits (path sides still sit on the wire)
- Live P2P Dandelion++
- Magister credential coordinator
- Cashu mint
- Hardware signing
- Explorer-visible amounts fully gone
- coinbase bool still a bundle fingerprint
