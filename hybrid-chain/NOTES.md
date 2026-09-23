# Living-set notes (`dev`)

- dest = sk·G. nf = Hash(sk || cm). Tickets cannot spend.
- Membership is a window path against live_root. No listed ring.
- Living set = stored window. Dropped notes are not spendable.
- Emission is an OR among padded outputs for schedule(h), not a dest-tagged point.
- Scan tags use eph_pk + diversifier. Asset is inside cm (generator A).
- Block codec is header + compact bundles only.
- One NoteProof object: SpendAuth + WindowPath + BindingSig.
