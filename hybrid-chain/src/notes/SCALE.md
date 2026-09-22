# Bounded-epoch membership (instead of unbounded FCMP++)

FCMP++ proves “this output is one of every output ever.” The proving tree and
prover work grow with the whole chain. That is the scale problem.

This crate seals the live note tree every `EPOCH_CAP` (2^16) leaves. A spend
carries a **fixed-size** `ScaleProof`:

- in-epoch path padded to 16 hashes
- forest path padded to 16 hashes
- 1098 bytes always, until 2^32 notes

Nodes keep sealed epoch **roots**, not every historical leaf. Wallets keep the
path for notes they own. Verifying a spend does not scan the chain.

Anonymity set per spend is the epoch (65 536 notes) plus “this epoch root is in
the forest.” Hiding `epoch_index` on the wire needs a circuit over the forest
peaks (next). Until then the proof still names the epoch; cost does not.

`notes_root` stays `live.root()` until the first epoch seals, so current tests
and `chain.bin` layout for short chains are unchanged.
