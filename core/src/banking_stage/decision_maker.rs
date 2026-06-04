//! Decision maker: decides whether to include or forward a packet.
//! In the hybrid chain there is no leader check — all nodes can produce blocks.

#[derive(Debug, Eq, PartialEq)]
pub enum BufferedPacketsDecision {
    /// Stage the packet for inclusion in the next block.
    Consume,
    /// Forward the packet to peers.
    Forward,
    /// Drop the packet (e.g., duplicate or fee too low).
    Drop,
}

pub struct DecisionMaker;

impl DecisionMaker {
    /// All nodes are potential block producers; always Consume.
    pub fn decide(_is_mining: bool, _queue_pressure: f64) -> BufferedPacketsDecision {
        BufferedPacketsDecision::Consume
    }
}
