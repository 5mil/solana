//! DEPRECATED: next_leader.rs — leader schedule removed in hybrid PoW/PoS.
//! In the hybrid chain, any node that mines or mints a valid block becomes
//! the block producer. There is no pre-assigned leader rotation.
//! This stub is retained for import-path compatibility during transition.

use solana_sdk::pubkey::Pubkey;

/// Always returns `None` — no leader schedule in hybrid PoW/PoS.
/// Retained for ABI compatibility only.
pub fn next_leader_tpu(
    _cluster_info: &std::sync::Arc<solana_gossip::cluster_info::ClusterInfo>,
    _poh_recorder: &std::sync::RwLock<()>,
) -> Option<(Pubkey, std::net::SocketAddr)> {
    None
}
