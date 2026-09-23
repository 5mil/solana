//! Miner payout is a spend pubkey from a wallet seed, not a ticket string.

use super::keys::{ScanKey, SpendKey, SpendPk};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct SealedPayout {
    pub dest: SpendPk,
    pub spend: SpendKey,
    pub scan: ScanKey,
}

impl SealedPayout {
    /// `wallet_seed` must be a wallet secret. Do not pass a pool ticket.
    pub fn from_wallet_seed(wallet_seed: &[u8], height: u64) -> Self {
        let mut spend_seed = b"payout-spend".to_vec();
        spend_seed.extend_from_slice(wallet_seed);
        spend_seed.extend_from_slice(&height.to_le_bytes());
        let mut scan_seed = b"payout-scan".to_vec();
        scan_seed.extend_from_slice(wallet_seed);
        scan_seed.extend_from_slice(&height.to_le_bytes());
        let spend = SpendKey::from_wallet_seed(&spend_seed);
        let scan = ScanKey::from_wallet_seed(&scan_seed);
        Self {
            dest: spend.pk(),
            spend,
            scan,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dest_is_not_the_ticket() {
        let p = SealedPayout::from_wallet_seed(b"wallet-secret", 1);
        assert_ne!(&p.dest.bytes[..], b"wallet-secret");
        assert_ne!(&p.dest.bytes[..], b"miner");
    }
}
