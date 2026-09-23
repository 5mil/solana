# Compact notes on hybrid-chain (`dev`)

Privacy is the only legal path. The spend does not republish the note.

## Forced

1. Spend = nullifier + rerandomized C′ + ring-OR link. Tree leaf is not the wire object.
2. Epoch pads are Ristretto points from uniform bytes (no known opening).
3. Emission blinding is dest+height, not height-only. Same-height rewards are not one point.
4. 48-bit range proofs on spends, outputs, fees, emission.
5. `header.notes_root = LaunchSet.commitment()` (window || fold || profile id).
6. History stays spendable. No refresh holiday.
7. One spend object. `transfer_bundle` / membership / dummy / coinbase flags gone from the legal path.
8. `RelayNet` stems then fluffs; origin is not the first hop id.
9. Binding signature: pk = ΣCin − ΣCout − Cfee.
10. `mint_pos_block(staker, &StakeProof)` — coins skipped on serde.

Classic `TxOutput.value` remains 0.
