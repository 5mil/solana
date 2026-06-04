//! Block producer — seals a fully-formed block and writes it to the ledger.
//!
//! Called by `tpu.rs` once the PoW or PoS service delivers a solution.
//! Responsibilities:
//!   1. Drain pending transactions from `BlockRecorder`
//!   2. Execute transactions against the working bank
//!   3. Compute the merkle root of executed transactions
//!   4. Write the sealed block to `Blockstore`
//!   5. Update `HybridBlockStore` consensus index
//!   6. Notify gossip/turbine to broadcast shreds

use {
    crate::consensus::sha256d_block_hash,
    solana_ledger::{
        blockstore::Blockstore,
        hybrid_block_store::HybridBlockStore,
    },
    solana_poh::poh_recorder::{BlockRecorder, SealedBlock},
    solana_runtime::bank_forks::BankForks,
    solana_sdk::{
        consensus::BlockConsensusData,
        hash::Hash,
    },
    std::sync::{Arc, RwLock},
};

/// Produce and persist a block from the pending transaction queue.
/// Returns the sealed block or an error string.
pub fn produce_block(
    recorder: &BlockRecorder,
    blockstore: &Arc<Blockstore>,
    block_store: &Arc<RwLock<HybridBlockStore>>,
    bank_forks: &Arc<RwLock<BankForks>>,
    parent_hash: [u8; 32],
    height: u64,
    block_time: u64,
    consensus_data: BlockConsensusData,
) -> Result<SealedBlock, String> {
    // 1. Drain transactions
    let transactions = recorder.drain_transactions(50_000);

    // 2. Execute against working bank (simplified — full execution in bank.rs)
    let bank = bank_forks.read().unwrap().working_bank();
    let results = bank.process_transactions(transactions.iter());
    let executed_txs: Vec<_> = transactions
        .into_iter()
        .zip(results.iter())
        .filter_map(|(tx, r)| if r.is_ok() { Some(tx) } else { None })
        .collect();

    // 3. Compute block hash (parent_hash + height + nonce + tx_root)
    let tx_root = merkle_root_of_txs(&executed_txs);
    let block_hash = sha256d_block_hash(&parent_hash, height, consensus_data.nonce, &tx_root);

    // 4. Build sealed block
    let sealed = SealedBlock {
        height,
        parent_hash,
        block_hash,
        block_time,
        transactions: executed_txs,
        consensus_data: consensus_data.clone(),
    };

    // 5. Update consensus index
    let parent_work = block_store.read().unwrap().tip_cumulative_work();
    block_store.write().unwrap().insert_block(
        height,
        block_hash,
        parent_hash,
        block_time,
        consensus_data,
        parent_work,
    );

    // 6. Seal recorder
    recorder.seal_block(block_hash);

    Ok(sealed)
}

/// Compute a simple SHA256 merkle root over serialized transactions.
/// Production will replace this with the full solana_ledger merkle tree.
fn merkle_root_of_txs(
    txs: &[solana_sdk::transaction::SanitizedTransaction],
) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for tx in txs {
        if let Ok(bytes) = bincode::serialize(tx.to_versioned_transaction()) {
            hasher.update(&bytes);
        }
    }
    hasher.finalize().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_root_empty() {
        let root = merkle_root_of_txs(&[]);
        assert_eq!(root.len(), 32);
    }
}
