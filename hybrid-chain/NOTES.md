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
- **Bounded-epoch forest** (`src/notes/epoch.rs`, `SCALE.md`): constant-size ScaleProof (1098 bytes), epoch cap 2^16 so prover work does not grow with full-chain |notes|

## Still not production-complete

- Halo 2 / zk-token-sdk range circuits
- Hiding epoch_index inside a circuit over forest peaks
- Live P2P Dandelion++
- Magister credential coordinator
- Cashu mint
- Hardware signing
- Explorer-visible amounts fully gone
- coinbase bool still a bundle fingerprint
