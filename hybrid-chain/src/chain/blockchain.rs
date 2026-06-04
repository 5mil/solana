use std::collections::HashMap;
use chrono::Utc;
use crate::chain::block::{Block, BlockHeader, BlockType};
use crate::chain::transaction::Transaction;
use crate::consensus::pow;
use crate::consensus::pos;
use crate::consensus::difficulty;
use crate::params::CHAIN_PARAMS;

pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub block_index: HashMap<[u8; 32], usize>,
    pub current_difficulty: u32,
    pub total_supply: u64,
}

impl Blockchain {
    /// Initialize with the genesis block
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

    /// Mine a new PoW block (SHA256d)
    pub fn mine_pow_block(&mut self, miner_address: &str) -> Block {
        let prev_hash = self.tip_hash();
        let height = self.height();
        let reward = self.current_pow_reward();
        let coinbase = Transaction::coinbase(height, reward, miner_address);
        let txs = vec![coinbase];
        let merkle = Block::compute_merkle_root(&txs);
        let now = Utc::now().timestamp();

        // Build header bytes for mining (without nonce)
        let proto_header = BlockHeader {
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
        let header_bytes = bincode::serialize(&proto_header).unwrap_or_default();
        let (nonce, _hash) = pow::mine(&header_bytes, self.current_difficulty);

        let final_header = BlockHeader { nonce, ..proto_header };
        let block = Block { header: final_header, transactions: txs };
        let block_hash = block.hash();

        self.block_index.insert(block_hash, self.blocks.len());
        self.total_supply += reward;
        self.blocks.push(block.clone());
        self.maybe_retarget();

        log::info!("PoW block #{} mined: {} (nonce={})", height, hex::encode(block_hash), nonce);
        block
    }

    /// Mint a new PoS block (coin-age based staking)
    pub fn mint_pos_block(&mut self, staker_address: &str, stake_coins: u64) -> Result<Block, &'static str> {
        let seconds_held: u64 = CHAIN_PARAMS.pos_coin_age_min + 3600; // assume mature stake
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
            nonce: 0, // PoS doesn't use nonce
            stake_modifier: [0u8; 32], // TODO: derive from stake kernel
        };

        let block = Block { header, transactions: txs };
        let block_hash = block.hash();

        self.block_index.insert(block_hash, self.blocks.len());
        self.total_supply += reward;
        self.blocks.push(block.clone());

        log::info!("PoS block #{} minted: {} (reward={})", height, hex::encode(block_hash), reward);
        Ok(block)
    }

    /// Halving schedule: reward halves every 210,000 blocks (Bitcoin-style)
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
