# Launch membership vs full-chain proofs

Full-chain membership proves “this output is one of every output ever.”
Prover work and node RAM grow with the chain.

## What launch does

1. **Hide the window.** `HiddenProof` has leaf + sibling path + side bits.
   No `epoch_index`, no `epoch_root`. The tree is the **sorted** union of
   the last W sealed epochs plus live. Rank is not insertion time.
2. **Keep epochs populated.** `seal()` pads live to the profile bucket with
   deterministic dummy leaves. Quiet networks do not publish short epochs.
   `ProfileKind::recommend` maps load + RAM → Constrained / Sparse /
   Standard / Dense.
3. **Fold.** Each seal: `forest_acc = H(forest_acc || epoch_root)`.
   Header commitment = `H(window_root || forest_acc)` (32 bytes).
   Notes that leave the window must refresh. There is no proof against
   all of history, so verify does not grow with height.

## Proof size

`HiddenProof` is always `32 + 4 + 20*32 + 1 = 677` bytes, any profile.

## Cutover

`transfer_window_bundle` is the launch spend constructor.
`Blockchain.notes` still feeds the older live-tree path until persist
rebuild is switched to `LaunchSet`.
