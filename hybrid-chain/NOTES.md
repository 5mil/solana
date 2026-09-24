# Living-set notes (`dev`)

- ImageOr covers every live leaf. The spend does not list a ring. `sample_ring` is gone.
- Conservation is BindingSig: ΣC = rH with r ≠ 0.
- Consensus calls NoteProof.verify with every real output, not outs.first().
- mine_pow_block(wallet_seed) derives dest from that seed. Ticket ≠ seed.
- Emission pad dest comes from the emission opening r.
- Range proofs are still a 48-bit sigma. That is not Bulletproofs.
