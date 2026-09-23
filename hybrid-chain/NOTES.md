# Living-set notes (`dev`) — binding residual + window-wide membership

Different system, not a padded RingCT:

- ImageOr is over every live leaf. The spend does not list decoys.
- Conservation is BindingSig: ΣC = rH with r ≠ 0. Point-identity is rejected.
- Every conserved C is ranged (C′, each output, fee). No dummy conservation leg.
- Emission pad dest comes from the emission opening r, not the miner dest.
- mine_pow_block(wallet_seed) derives dest from that seed, never from the tip.
- Production mining is mine_pow_with_payout(ticket, &SealedPayout).
