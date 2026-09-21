use std::collections::HashMap;
use chrono::Utc;
use crate::chain::block::{Block, BlockHeader, BlockType};
use crate::chain::transaction::Transaction;
use crate::consensus::pow;
use crate::consensus::pos;
use crate::consensus::difficulty;
use crate::params::CHAIN_PARAMS;

#[derive(Debug)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub block_index: HashMap<[u8; 32], usize>,
    pub current_difficulty: u32,
    pub total_supply: u64,
}

impl Blockchain {
    pub fn new() -> Self {
        let mut chain = Blockchain {
            blocks: Vec::new(),
            block_index: HashMap::new(),
            current_difficulty: CHAIN_PARAMS.initial_difficulty,
            total_supply: 0,
        };
        chain.create_genesis();
        chain
    }

    fn create_genesis(&mut self) {
        let genesis_tx = Transaction::coinbase(0, CHAIN_PARAMS.pow_block_reward, "genesis");
        let txs = vec![genesis_tx];
        let merkle = Block::compute_merkle_root(&txs);

        let header = BlockHeader {
            version: 1,
            height: 0,
            prev_hash: [0u8; 32],
            merkle_root: merkle,
            timestamp: CHAIN_PARAMS.genesis_timestamp,
            difficulty: self.current_difficulty,
            block_type: BlockType::PoW,
            nonce: 0,
            stake_modifier: [0u8; 32],
        };

        let genesis = Block { header, transactions: txs };
        let hash = genesis.hash();
        self.block_index.insert(hash, 0);
        self.total_supply += CHAIN_PARAMS.pow_block_reward;
        self.blocks.push(genesis);
        log::info!("Genesis block created: {}", hex::encode(hash));
    }

    pub fn tip_hash(&self) -> [u8; 32] {
        self.blocks.last().map(|b| b.hash()).unwrap_or([0u8; 32])
    }

    pub fn height(&self) -> u64 {
        self.blocks.len() as u64
    }

    pub fn mine_pow_block(&mut self, miner_address: &str) -> Block {
        let prev_hash = self.tip_hash();
        let height = self.height();
        let reward = self.current_pow_reward();
        let coinbase = Transaction::coinbase(height, reward, miner_address);
        let txs = vec![coinbase];
        let merkle = Block::compute_merkle_root(&txs);
        let now = Utc::now().timestamp();

        let mut header = BlockHeader {
            version: 1,
            height,
            prev_hash,
            merkle_root: merkle,
            timestamp: now,
            difficulty: self.current_difficulty,
            block_type: BlockType::PoW,
            nonce: 0,
            stake_modifier: [0u8; 32],
        };

        let mut nonce: u64 = 0;
        loop {
            header.nonce = nonce;
            let hash = pow::sha256d(&bincode::serialize(&header).unwrap_or_default());
            if pow::meets_difficulty(&hash, self.current_difficulty) {
                break;
            }
            nonce = nonce.wrapping_add(1);
        }

        let block = Block { header, transactions: txs };
        let block_hash = block.hash();

        self.block_index.insert(block_hash, self.blocks.len());
        self.total_supply += reward;
        self.blocks.push(block.clone());
        self.maybe_retarget();

        log::info!("PoW block #{} mined: {} (nonce={})", height, hex::encode(block_hash), nonce);
        block
    }

    pub fn mint_pos_block(&mut self, staker_address: &str, stake_coins: u64) -> Result<Block, &'static str> {
        let seconds_held: u64 = CHAIN_PARAMS.pos_coin_age_min + 3600;
        pos::validate_stake(stake_coins, seconds_held)?;

        let reward = pos::pos_reward(stake_coins, seconds_held);
        let prev_hash = self.tip_hash();
        let height = self.height();
        let coinstake = Transaction::coinstake(stake_coins, reward, staker_address);
        let txs = vec![coinstake];
        let merkle = Block::compute_merkle_root(&txs);
        let now = Utc::now().timestamp();

        let header = BlockHeader {
            version: 1,
            height,
            prev_hash,
            merkle_root: merkle,
            timestamp: now,
            difficulty: self.current_difficulty,
            block_type: BlockType::PoS,
            nonce: 0,
            stake_modifier: [0u8; 32],
        };

        let block = Block { header, transactions: txs };
        let block_hash = block.hash();

        self.block_index.insert(block_hash, self.blocks.len());
        self.total_supply += reward;
        self.blocks.push(block.clone());

        log::info!("PoS block #{} minted: {} (reward={})", height, hex::encode(block_hash), reward);
        Ok(block)
    }

    pub fn current_pow_reward(&self) -> u64 {
        let halvings = self.height() / 210_000;
        if halvings >= 64 { return 0; }
        CHAIN_PARAMS.pow_block_reward >> halvings
    }

    fn maybe_retarget(&mut self) {
        let window = CHAIN_PARAMS.difficulty_adjustment_window;
        if self.height() % window == 0 && self.height() > 0 {
            let window_start = (self.height() - window) as usize;
            let first_ts = self.blocks[window_start].header.timestamp;
            let last_ts = self.blocks.last().unwrap().header.timestamp;
            let actual = (last_ts - first_ts).max(1) as u64;
            let target = difficulty::pow_target_timespan();
            self.current_difficulty = difficulty::retarget(self.current_difficulty, actual, target);
            log::info!("Difficulty retarget: {} bits", self.current_difficulty);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::pow::{meets_difficulty, sha256d};

    #[test]
    fn genesis_is_height_one_tip_after_pow() {
        let mut chain = Blockchain::new();
        assert_eq!(chain.height(), 1);
        let block = chain.mine_pow_block("miner");
        assert_eq!(block.header.height, 1);
        assert_eq!(chain.height(), 2);
        let bytes = bincode::serialize(&block.header).unwrap();
        let hash = sha256d(&bytes);
        assert!(meets_difficulty(&hash, block.header.difficulty));
        assert_eq!(block.hash(), hash);
    }

    #[test]
    fn pos_rejects_below_min_stake() {
        let mut chain = Blockchain::new();
        assert!(chain.mint_pos_block("staker", 1).is_err());
    }

    #[test]
    fn pos_accepts_min_stake() {
        let mut chain = Blockchain::new();
        let block = chain
            .mint_pos_block("staker", CHAIN_PARAMS.min_stake)
            .expect("pos mint");
        assert_eq!(block.header.block_type, BlockType::PoS);
        assert_eq!(block.header.height, 1);
    }
}
