//! Compact note infrastructure for hybrid-chain.
//!
//! Phase 1–3 foundation:
//! - Pedersen-style value commitments (Ristretto)
//! - Dummy-padded actions with conservation
//! - Append-only note commitment tree
//! - Spend-tag set (double-spend)
//! - Sealed miner payout destinations (pool must not bind worker id)

pub mod action;
pub mod commitment;
pub mod payout;
pub mod tags;
pub mod tree;

pub use action::{ActionBundle, CompactAction, CompactOutput, CompactSpend};
pub use commitment::{
    blinding_from_seed, verify_balance, PedersenGenerators, ValueCommitment,
};
pub use payout::{SealedPayout, discovery_tag};
pub use tags::SpendTagSet;
pub use tree::NoteCommitmentTree;
