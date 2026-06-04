//! REPLACED: PohService has been superseded by PowMiningService + PosMintingService.
//! This stub is retained so crates that import `poh::poh_service` still compile
//! during the transition. No threads are started; no PoH hashing occurs.

/// No-op replacement for the legacy PoH service.
/// Real block production happens in `core::pow_service` and `core::pos_service`.
pub struct PohService;

impl PohService {
    /// Construct a no-op PohService. All arguments are ignored.
    pub fn new<T>(_poh_recorder: T, _config: (), _exit: std::sync::Arc<std::sync::atomic::AtomicBool>, _hashes_per_batch: u64) -> Self {
        Self
    }

    pub fn join(self) -> std::thread::Result<()> {
        Ok(())
    }
}
