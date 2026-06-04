//! DEPRECATED: PoH timing report service.
//! PoH has been replaced by SHA256d PoW/PoS hybrid consensus.
//! This stub is retained to avoid breaking any lingering import paths.

/// No-op PoH timing report service.
pub struct PohTimingReportService;

impl PohTimingReportService {
    /// Create a no-op instance. Does nothing in the hybrid consensus system.
    pub fn new(_receiver: std::sync::mpsc::Receiver<()>) -> Self {
        Self
    }

    pub fn join(self) -> std::thread::Result<()> {
        Ok(())
    }
}
