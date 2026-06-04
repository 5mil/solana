//! DEPRECATED: PoH timing reporter.
//! PoH has been replaced by SHA256d PoW/PoS hybrid consensus.
//! This stub is retained to avoid breaking any lingering import paths.

/// No-op struct replacing the PoH timing reporter.
pub struct PohTimingReporter;

impl Default for PohTimingReporter {
    fn default() -> Self {
        Self
    }
}

impl PohTimingReporter {
    pub fn new() -> Self {
        Self
    }

    /// No-op — PoH timing no longer tracked.
    pub fn process_poh_timing_point(
        &mut self,
        _slot: u64,
        _poh_timing_point: (),
    ) -> bool {
        false
    }
}
