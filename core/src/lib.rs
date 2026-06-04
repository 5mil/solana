//! Solana Core — modified for SHA256d PoW/PoS hybrid consensus.
//! Tower BFT, PoH service, and leader schedule have been removed
//! from the consensus path. Block production is driven by PoW mining
//! (pow_service) and PoS minting (pos_service).

#![cfg_attr(RUSTC_WITH_SPECIALIZATION, feature(min_specialization))]

pub mod accounts_hash_verifier;
pub mod admin_rpc_post_init;
pub mod banking_stage;
pub mod banking_trace;
pub mod cache_block_meta_service;
/// Hybrid SHA256d PoW/PoS consensus engine (replaces Tower BFT)
pub mod consensus;
pub mod commitment_service;
pub mod completed_data_sets_service;
pub mod cost_update_service;
pub mod drop_bank_service;
pub mod fetch_stage;
pub mod gen_keys;
pub mod next_leader;
pub mod optimistic_confirmation_verifier;
/// DEPRECATED PoH timing stubs (retained for ABI compat)
pub mod poh_timing_report_service;
pub mod poh_timing_reporter;
/// SHA256d PoW mining service
pub mod pow_service;
/// PoS coin-age minting service
pub mod pos_service;
pub mod repair;
pub mod replay_stage;
pub mod result;
pub mod rewards_recorder_service;
pub mod sample_performance_service;
pub mod shred_fetch_stage;
pub mod sigverify;
pub mod sigverify_stage;
pub mod snapshot_packager_service;
pub mod staked_nodes_updater_service;
pub mod stats_reporter_service;
pub mod system_monitor_service;
pub mod tpu;
pub mod tpu_entry_notifier;
pub mod tracer_packet_stats;
pub mod tvu;
pub mod unfrozen_gossip_verified_vote_hashes;
pub mod validator;
pub mod verified_vote_packets;
pub mod vote_simulator;
pub mod voting_service;
pub mod warm_quic_cache_service;
pub mod window_service;

/// Re-export primary consensus types for convenience.
pub use consensus::{
    ChainTip,
    ConsensusError,
    HybridConsensusEngine,
    MinerConfig,
    sha256d_block_hash,
    pos_kernel_hash,
};
pub use pow_service::{PowMiningService, SolvedPowBlock};
pub use pos_service::{PosMintingService, MintedPosBlock, StakeUtxo};
