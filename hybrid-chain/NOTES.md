# Compact notes on hybrid-chain (`dev`)

Privacy is the only legal path. Consensus rejects the old exits.

## Forced by algebra / validation

- Classic `TxOutput.value` must be 0. Persist refuses any plaintext amount.
- Spends must carry `HiddenProof`. `MembershipProof` on a real spend is `membership path forbidden`.
- Emission is a no-spend padded bundle. Extra emission in user space is rejected.
- PoW reward is checked as `Commit(schedule(height), emission_blinding(height))`, not a visible number.
- Real vs pad is identity commitment / zero tag, not a trusted `dummy` flag.
- Window membership: leaf must be in `LaunchSet` or the spend dies.

## Still on the wire (not an opt-out, still a leak to close)

- Merkle path side bits
- `dummy` / `coinbase` fields exist on the struct but are ignored by verification
- Range proofs not yet in-circuit
