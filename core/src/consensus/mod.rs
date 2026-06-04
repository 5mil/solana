//! Consensus sub-modules.
//! The Tower BFT implementation has been removed.
//! Chain state tracking lives in `core::consensus` (the parent module).
//! This directory is retained for any future consensus sub-modules.

// Re-export the engine from the parent for backwards compat with any
// internal imports that used `crate::consensus::tower_storage` etc.
pub use crate::consensus::{
    ChainTip, ConsensusError, HybridConsensusEngine, MinerConfig,
    sha256d_block_hash, pos_kernel_hash,
};
