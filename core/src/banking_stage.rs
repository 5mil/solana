//! Banking stage — modified for hybrid PoW/PoS consensus.
//!
//! Incoming verified transactions are recorded into the `BlockRecorder`
//! (which replaced `PohRecorder`). The banking stage no longer gates
//! transaction processing on PoH ticks or leader schedule.
//!
//! Transaction execution happens in the block-finalizer thread (tpu.rs)
//! once the mining/minting service delivers a sealed block.

use {
    crossbeam_channel::{Receiver, RecvTimeoutError},
    solana_gossip::cluster_info::ClusterInfo,
    solana_perf::packet::PacketBatch,
    solana_poh::poh_recorder::BlockRecorder,
    solana_runtime::bank_forks::BankForks,
    solana_sdk::transaction::SanitizedTransaction,
    std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc, RwLock,
        },
        thread::{self, Builder, JoinHandle},
        time::Duration,
    },
};

// ---------------------------------------------------------------------------
// BankingStage
// ---------------------------------------------------------------------------

pub struct BankingStage {
    bank_thread_hdls: Vec<JoinHandle<()>>,
}

const NUM_BANKING_THREADS: usize = 4;
/// Max time to wait on the verified-packet channel per iteration.
const RECV_TIMEOUT: Duration = Duration::from_millis(10);

impl BankingStage {
    pub fn new(
        cluster_info: Arc<ClusterInfo>,
        verified_receiver: Receiver<Vec<PacketBatch>>,
        bank_forks: Arc<RwLock<BankForks>>,
        block_recorder: BlockRecorder,
        exit: Arc<AtomicBool>,
    ) -> Self {
        // Share the receiver across threads via Arc<Mutex>
        let shared_rx = Arc::new(std::sync::Mutex::new(verified_receiver));

        let bank_thread_hdls: Vec<JoinHandle<()>> = (0..NUM_BANKING_THREADS)
            .map(|i| {
                let shared_rx = Arc::clone(&shared_rx);
                let recorder = block_recorder.clone_handle();
                let bank_forks = Arc::clone(&bank_forks);
                let exit = Arc::clone(&exit);
                Builder::new()
                    .name(format!("solBanking{i:02}"))
                    .spawn(move || {
                        Self::banking_loop(shared_rx, recorder, bank_forks, exit);
                    })
                    .expect("spawn banking thread")
            })
            .collect();

        Self { bank_thread_hdls }
    }

    fn banking_loop(
        shared_rx: Arc<std::sync::Mutex<Receiver<Vec<PacketBatch>>>>,
        recorder: BlockRecorder,
        bank_forks: Arc<RwLock<BankForks>>,
        exit: Arc<AtomicBool>,
    ) {
        while !exit.load(Ordering::Relaxed) {
            let batches = {
                let rx = shared_rx.lock().unwrap();
                match rx.recv_timeout(RECV_TIMEOUT) {
                    Ok(b) => b,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            };

            let bank = bank_forks.read().unwrap().working_bank();

            for batch in &batches {
                for packet in batch.iter() {
                    if packet.meta().discard() {
                        continue;
                    }
                    // Deserialize and sanitize the transaction.
                    if let Ok(tx) = packet.deserialize_slice::<SanitizedTransaction, _>(..) {
                        recorder.record_transaction(tx);
                    }
                }
            }
        }
    }

    pub fn join(self) -> thread::Result<()> {
        for hdl in self.bank_thread_hdls {
            hdl.join()?;
        }
        Ok(())
    }
}
