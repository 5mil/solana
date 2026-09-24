# Programmed notes

Value still moves as `ActionBundle`. Programs are predicates on notes, not a
second transaction type and not an account VM.

## Birth

`CompactOutput.pred` is `{id, commit}`. Default (zeros) is spend-key only.
Listed ids: `pk`, `pk-n`, `after`, `and`, `or`, `rate`, `swap`, `pool-lp`.
Unknown ids fail closed.

The living set stores `LiveNote { cm, pred, pred_commit }`. Pads are default
policy. `window_root` is still the merkle of commitments.

## Spend

`CompactSpend` carries the same header, the predicate body, and a witness.
`ImageOr` runs over `live_leaves_for(id, commit)` — only notes born with that
policy. An `after` note is not a decoy for a default spend.

`verify_programs(height)` runs after conservation and `NoteProof`.

## Intents

`ActionBundle.intents` + `fills`. The node does not match. It checks expiry
and that each intent has a fill index into this bundle. Empty is the normal
transfer path.

## Exec

`exec: Option<ExecProof>`. Only `program_id = 0` with an empty proof is
listed (native predicates). Any other program id is rejected. That is the
hook for a later proof-carrying program without adding a parallel body.
