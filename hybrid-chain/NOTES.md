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
- Bounded-epoch forest (`epoch.rs`) — capped prover work
- **Launch set** (`launch.rs`): hidden window proof, adaptive profiles, pad-to-bucket, folded 32-byte acc, refresh-or-drop

## Launch vs full-chain membership

- Spends use `HiddenProof` (no `epoch_index` / `epoch_root` on the wire).
- Window leaves are **sorted** before the Merkle root, so path rank is not age.
- Seal pads every epoch to the profile bucket — sparse and dense look the same.
- `ProfileKind::recommend(notes/hour, ram)` picks Constrained / Sparse / Standard / Dense.
- `forest_acc` folds each sealed epoch; commitment is `H(window_root || forest_acc)`.
- Notes that roll off the window must refresh into live. Verify stays bounded at any chain length.

## Still not production-complete

- Halo 2 / zk-token-sdk range circuits (path sides still sit on the wire)
- Live P2P Dandelion++
- Magister credential coordinator
- Cashu mint
- Hardware signing
- Explorer-visible amounts fully gone
- coinbase bool still a bundle fingerprint
- Blockchain.notes still the live tree; LaunchSet is the spend path to cut over
