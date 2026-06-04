//! DEPRECATED: next_leader.rs was part of the PoH/leader-schedule system.
//! In hybrid PoW/PoS consensus there is no rotating leader — block production
//! is open to any node that wins the PoW race or satisfies PoS coin-age.
//! This stub is retained for import-path compatibility.

use solana_sdk::pubkey::Pubkey;

/// No-op: returns None. No leader schedule in hybrid PoW/PoS.
pub fn next_leader_tpu(
    _cluster_info: &std::sync::Arc<solana_gossip::cluster_info::ClusterInfo>,
    _poh_recorder: &std::sync::RwLock<()>,
) -> Option<(Pubkey, std::net::SocketAddr)> {
    None
}
