//! Packet receiver: drains crossbeam channels and feeds the priority queue.

use {
    crate::banking_stage::TransactionPriorityQueue,
    crossbeam_channel::{Receiver, TryRecvError},
    solana_sdk::packet::Packet,
    std::sync::{Arc, RwLock},
};

pub struct PacketReceiver {
    queue: Arc<RwLock<TransactionPriorityQueue>>,
}

impl PacketReceiver {
    pub fn new(queue: Arc<RwLock<TransactionPriorityQueue>>) -> Self { Self { queue } }

    /// Drain all pending packets from the channel into the priority queue.
    /// Fee priority defaults to 0 until full fee calculation is wired in.
    pub fn drain_channel(&self, rx: &Receiver<Vec<Packet>>) -> usize {
        let mut count = 0;
        loop {
            match rx.try_recv() {
                Ok(_packets) => {
                    // TODO: deserialize + sigverify + fee-extract + push to queue
                    count += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
        }
        count
    }
}
