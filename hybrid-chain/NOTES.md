# Living-set notes (`dev`) — post-review executive cut

Addressed from the independent review:

- Dest is not on SpendAuth / NoteProof. Tag is a key image I = sk·Hp(cm).
- ImageOr binds I and C′ to a window-only ring (C′ is a rerandomization of one live cm).
- Ranges sit inside NoteProof (in/out/fee).
- Fiat–Shamir contexts include live_root, height, ring, image, C′.
- mine_pow_with_payout(ticket, wallet). Ticket is not the seed.
- Emission OR stores reward+height; slot is not hard-coded 0.
- Scan refuses non-canonical eph (try_point).
- Persist rebuilds PoS emission from proof.reward.
