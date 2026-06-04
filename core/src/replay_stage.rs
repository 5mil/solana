//! Replay stage — modified for hybrid PoW/PoS fork-choice.
//!
//! When two competing chain tips arrive, the replay stage selects the
//! heaviest chain by cumulative PoW work (Nakamoto rule). Tower BFT
//! vote-based fork choice has been removed entirely.
//!
//! The replay stage also calls `validate_block_consensus()` on every
//! incoming block before executing its transactions, rejecting blocks
//! whose PoW hash or PoS kernel is invalid.

use {
    crate::consensus::HybridConsensusEngine,
    solana_ledger::{
        blockstore::Blockstore,
        hybrid_block_store::HybridBlockStore,
    },
    solana_runtime::{
        bank_forks::BankForks,
        block_validator::{validate_block_consensus, BlockValidationError},
    },
    solana_sdk::{
        consensus::BlockConsensusData,
        genesis_config::GenesisConfig,
    },
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
// Config
// ---------------------------------------------------------------------------

pub struct ReplayStageConfig {
    pub block_store: Arc<RwLock<HybridBlockStore>>,
    pub engine: Arc<HybridConsensusEngine>,
    pub genesis_config: Arc<GenesisConfig>,
}

// ---------------------------------------------------------------------------
// Pending block: received from the network, not yet replayed
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct PendingBlock {
    pub height: u64,
    pub block_hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub block_time: u64,
    pub parent_time: u64,
    pub consensus_data: BlockConsensusData,
    pub extra_data: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Replay stage
// ---------------------------------------------------------------------------

pub struct ReplayStage {
    thread_hdl: JoinHandle<()>,
}

impl ReplayStage {
    pub fn new(
        config: ReplayStageConfig,
        blockstore: Arc<Blockstore>,
        bank_forks: Arc<RwLock<BankForks>>,
        exit: Arc<AtomicBool>,
    ) -> Self {
        let thread_hdl = Builder::new()
            .name("solReplayStage".to_string())
            .spawn(move || {
                Self::run(config, blockstore, bank_forks, exit);
            })
            .unwrap();
        Self { thread_hdl }
    }

    fn run(
        config: ReplayStageConfig,
        blockstore: Arc<Blockstore>,
        bank_forks: Arc<RwLock<BankForks>>,
        exit: Arc<AtomicBool>,
    ) {
        while !exit.load(Ordering::Relaxed) {
            // In production this would receive PendingBlocks from the window service.
            // For now we sleep and yield; the real intake is wired via channels in Tvu.
            thread::sleep(Duration::from_millis(50));
        }
    }

    /// Validate and apply a single incoming block.
    /// Returns `Err` if the block fails consensus validation and should be discarded.
    pub fn validate_and_apply(
        config: &ReplayStageConfig,
        pending: &PendingBlock,
    ) -> Result<(), BlockValidationError> {
        validate_block_consensus(
            &pending.block_hash,
            &pending.parent_hash,
            pending.height,
            pending.block_time,
            pending.parent_time,
            &pending.consensus_data,
            &config.genesis_config,
        )?;

        // Fork-choice: accept only if this block extends the heaviest chain
        // or IS the heaviest chain tip.
        let store = config.block_store.read().unwrap();
        let current_tip_work = store.tip_cumulative_work();
        // We compute candidate work as parent_work + block_work.
        let parent_rec = store.get_by_hash(&pending.parent_hash);
        let parent_work = parent_rec.map(|r| r.cumulative_work).unwrap_or(0);
        use solana_sdk::consensus::ConsensusType;
        let block_work: u128 = if pending.consensus_data.consensus_type == ConsensusType::ProofOfWork {
            1u128 << pending.consensus_data.difficulty_bits.min(127)
        } else {
            0
        };
        let candidate_work = parent_work.saturating_add(block_work);

        if candidate_work < current_tip_work {
            // This block is on a lighter chain — discard.
            return Err(BlockValidationError::PowHashInvalid {
                difficulty_bits: pending.consensus_data.difficulty_bits,
            });
        }

        Ok(())
    }

    pub fn join(self) -> thread::Result<()> {
        self.thread_hdl.join()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::consensus::{BlockConsensusData, ConsensusType};

    fn make_pending(bits: u32, consensus_type: ConsensusType, block_hash_byte: u8) -> PendingBlock {
        let mut block_hash = [0u8; 32];
        block_hash[0] = block_hash_byte;
        PendingBlock {
            height: 1,
            block_hash,
            parent_hash: [0u8; 32],
            block_time: 2000,
            parent_time: 1000,
            consensus_data: BlockConsensusData {
                consensus_type,
                nonce: 0,
                difficulty_bits: bits,
                stake_modifier: [0u8; 32],
                coin_age_consumed: 0,
            },
            extra_data: vec![],
        }
    }

    #[test]
    fn test_fork_choice_lighter_chain_rejected() {
        use crate::consensus::HybridConsensusEngine;
        use solana_sdk::consensus::HybridConsensusConfig;
        use std::sync::atomic::AtomicBool;
        use solana_ledger::hybrid_block_store::HybridBlockStore;

        let exit = Arc::new(AtomicBool::new(false));
        let engine = Arc::new(HybridConsensusEngine::new(
            HybridConsensusConfig::default(), exit.clone(),
        ));
        let mut store = HybridBlockStore::new();
        // Insert a heavy tip at height 1 with difficulty 24
        let heavy_hash = [1u8; 32];
        store.insert_block(1, heavy_hash, [0u8;32], 1000,
            BlockConsensusData { consensus_type: ConsensusType::ProofOfWork,
                nonce: 0, difficulty_bits: 24,
                stake_modifier: [0u8;32], coin_age_consumed: 0 }, 0);
        let block_store = Arc::new(RwLock::new(store));
        let genesis = Arc::new(GenesisConfig::default());
        let config = ReplayStageConfig { block_store, engine, genesis_config: genesis };

        // A lighter block (diff=1) extending from genesis should be rejected
        let light_block = make_pending(1, ConsensusType::ProofOfWork, 0x00);
        // It passes timestamp checks but loses fork-choice
        // (parent_work=0, block_work=2, candidate=2 < tip_work=2^24)
        let result = ReplayStage::validate_and_apply(&config, &light_block);
        assert!(result.is_err());
    }
}
