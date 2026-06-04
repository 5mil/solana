//! REPLACED: LeaderBankNotifier was part of the PoH/leader-schedule system.
//! In the hybrid PoW/PoS chain there is no leader rotation — any node that
//! finds a valid PoW nonce or PoS kernel becomes the block producer.
//! This stub is retained for import-path compatibility.

use std::sync::Arc;

/// No-op notifier replacing the leader bank notification system.
#[derive(Default, Clone)]
pub struct LeaderBankNotifier;

impl LeaderBankNotifier {
    pub fn new() -> Arc<Self> {
        Arc::new(Self)
    }

    /// No-op — leader schedule removed in hybrid consensus.
    pub fn set_in_leader_slot(&self, _in_leader: bool) {}

    /// No-op — always returns false; no concept of "leader" in PoW/PoS.
    pub fn is_in_leader_slot(&self) -> bool {
        false
    }
}
