//! DEPRECATED: Proof of History has been replaced by SHA256d PoW/PoS hybrid consensus.
//! This file is retained as a stub for ABI compatibility during transition.
//! See `consensus.rs` for the new consensus configuration.

use serde::{Deserialize, Serialize};

/// Legacy PoH config stub — no longer used in consensus.
/// Kept to avoid breaking downstream crate imports during migration.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct PohConfig {
    /// DEPRECATED: PoH target tick duration — not used in hybrid consensus
    pub target_tick_duration: std::time::Duration,
    /// DEPRECATED: PoH hashes per tick — not used in hybrid consensus
    pub hashes_per_tick: Option<u64>,
    /// DEPRECATED: target ticks per second — not used in hybrid consensus
    pub target_tick_count: Option<u64>,
}

impl Default for PohConfig {
    fn default() -> Self {
        Self {
            target_tick_duration: std::time::Duration::from_millis(0),
            hashes_per_tick: None,
            target_tick_count: None,
        }
    }
}
