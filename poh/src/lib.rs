//! `poh` crate — modified for SHA256d PoW/PoS hybrid consensus.
//! PohRecorder and PohService have been replaced by BlockRecorder,
//! PowMiningService (core::pow_service), and PosMintingService (core::pos_service).

/// Block recorder: stages transactions for the next mined/minted block.
/// Replaces PohRecorder.
pub mod poh_recorder;

/// No-op PoH service stub retained for ABI compatibility.
pub mod poh_service;

/// No-op leader bank notifier stub retained for ABI compatibility.
pub mod leader_bank_notifier;

/// Re-export the primary types.
pub use poh_recorder::{BlockRecorder, SealedBlock, PendingTransaction, MAX_PENDING_TXS};
pub use poh_service::PohService;
pub use leader_bank_notifier::LeaderBankNotifier;
