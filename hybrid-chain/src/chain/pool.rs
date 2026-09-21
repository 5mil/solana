//! Default local mining pool.
//!
//! Miners submit solved PoW headers here. The pool validates SHA256d
//! against the advertised difficulty and, on success, records the share
//! as accepted work for the default pool.

use crate::chain::block::{Block, BlockType};
use crate::consensus::pow::{meets_difficulty, sha256d};
use std::sync::{Arc, Mutex};

pub const DEFAULT_POOL_NAME: &str = "hybrid-default-pool";

#[derive(Debug, Clone)]
pub struct AcceptedShare {
    pub miner: String,
    pub height: u64,
    pub nonce: u64,
    pub hash: [u8; 32],
}

#[derive(Default)]
struct PoolInner {
    accepted: Vec<AcceptedShare>,
    rejected: u64,
}

#[derive(Clone, Default)]
pub struct DefaultPool {
    inner: Arc<Mutex<PoolInner>>,
}

#[derive(Debug)]
pub enum SubmitError {
    WrongBlockType,
    InsufficientWork,
    Serialize,
}

impl DefaultPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name() -> &'static str {
        DEFAULT_POOL_NAME
    }

    pub fn submit_block(&self, miner: &str, block: &Block) -> Result<AcceptedShare, SubmitError> {
        if block.header.block_type != BlockType::PoW {
            let mut inner = self.inner.lock().unwrap();
            inner.rejected += 1;
            return Err(SubmitError::WrongBlockType);
        }
        let bytes = bincode::serialize(&block.header).map_err(|_| SubmitError::Serialize)?;
        let hash = sha256d(&bytes);
        if !meets_difficulty(&hash, block.header.difficulty) {
            let mut inner = self.inner.lock().unwrap();
            inner.rejected += 1;
            return Err(SubmitError::InsufficientWork);
        }
        let share = AcceptedShare {
            miner: miner.to_string(),
            height: block.header.height,
            nonce: block.header.nonce,
            hash,
        };
        self.inner.lock().unwrap().accepted.push(share.clone());
        Ok(share)
    }

    pub fn accepted_count(&self) -> usize {
        self.inner.lock().unwrap().accepted.len()
    }

    pub fn rejected_count(&self) -> u64 {
        self.inner.lock().unwrap().rejected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chain::blockchain::Blockchain;

    #[test]
    fn default_pool_accepts_mined_pow_block() {
        let mut chain = Blockchain::new();
        let block = chain.mine_pow_block("pool-miner");
        let pool = DefaultPool::new();
        let share = pool.submit_block("pool-miner", &block).expect("share accepted");
        assert_eq!(share.height, 1);
        assert_eq!(pool.accepted_count(), 1);
        assert_eq!(pool.rejected_count(), 0);
    }

    #[test]
    fn default_pool_rejects_unsolved_header() {
        let mut chain = Blockchain::new();
        let mut block = chain.mine_pow_block("pool-miner");
        block.header.nonce = block.header.nonce.wrapping_add(1);
        let pool = DefaultPool::new();
        assert!(pool.submit_block("pool-miner", &block).is_err());
        assert_eq!(pool.rejected_count(), 1);
    }
}
