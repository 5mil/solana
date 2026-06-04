//! `solana-core` — hybrid PoW/PoS blockchain node.

#[macro_use]
extern crate log;

#[macro_use]
extern crate solana_metrics;

pub mod banking_stage;
pub mod banking_trace;
pub mod broadcast_stage;
/// Hybrid SHA256d PoW / Proof-of-Stake consensus engine.
pub mod consensus;
pub mod fetch_stage;
pub mod next_leader;
/// PoW mining service (replaces PoH ticking).
pub mod pow_service;
/// PoS minting service.
pub mod pos_service;
/// Replay stage (validates & commits blocks).
pub mod replay_stage;
pub mod sigverify;
pub mod sigverify_stage;
pub mod staked_nodes_updater_service;
pub mod tpu;
pub mod validator;

pub use consensus::{HybridConsensusEngine, MinerConfig, ConsensusError};
pub use pow_service::{PowMiningService, SolvedPowBlock};
pub use pos_service::{PosMintingService, MintedPosBlock, StakeUtxo};
pub use replay_stage::ReplayStage;
