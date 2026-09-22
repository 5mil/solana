use std::collections::HashMap;
use chrono::Utc;
use crate::chain::block::{Block, BlockHeader, BlockType};
use crate::chain::transaction::Transaction;
use crate::consensus::pow;
use crate::consensus::pos;
use crate::consensus::difficulty;
use crate::notes::action::{coinbase_bundle, ActionBundle};
use crate::notes::commitment::{blinding_from_seed, PedersenGenerators, ValueCommitment};
use crate::notes::payout::SealedPayout;
use crate::notes::tags::SpendTagSet;
use crate::notes::tree::NoteCommitmentTree;
use crate::params::CHAIN_PARAMS;

#[derive(Debug)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub block_index: HashMap<[u8; 32], usize>,
    pub current_difficulty: u32,
    pub total_supply: u64,
    pub notes: NoteCommitmentTree,
    pub tags: SpendTagSet,
}

impl Blockchain {
    pub fn new() -> Self {
        let mut chain = Blockchain {
            blocks: Vec::new(),
            block_index: HashMap::new(),
            current_difficulty: CHAIN_PARAMS.initial_difficulty,
            total_supply: 0,
            notes: NoteCommitmentTree::new(),
            tags: SpendTagSet::new(),
        };
        chain.create_genesis();
        chain
    }

    fn append_bundle(&mut self, bundle: &ActionBundle) -> Result<(), &'static str> {
        if !bundle.verify_conservation() {
            return Err("conservation failed");
        }
        for tag in bundle.spend_tags() {
            self.tags.insert(tag)?;
        }
        for c in bundle.output_commitments() {
            self.notes.append(c);
        }
        Ok(())
    }

    fn emission_bundle(&self, ticket: &str, height: u64, reward: u64) -> ActionBundle {
        let sealed = SealedPayout::from_ticket(ticket.as_bytes(), height);
        let gens = PedersenGenerators::default();
        let blind = blinding_from_seed(&[ticket.as_bytes(), &height.to_le_bytes()].concat());
        let commit = ValueCommitment::commit(reward, &blind, &gens);
        coinbase_bundle(sealed.dest, &sealed.scan_seed, commit, [0u8; 32])
    }

    fn create_genesis(&mut self) {
        let genesis_tx = Transaction::coinbase(0, CHAIN_PARAMS.pow_block_reward, "genesis");
        let txs = vec![genesis_tx];
        let merkle = Block::compute_merkle_root(&txs);
        let bundle = self.emission_bundle("genesis", 0, CHAIN_PARAMS.pow_block_reward);
        self.append_bundle(&bundle).expect("genesis notes");
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
            notes_root: self.notes.root(),
            tags_root: self.tags.root(),
        };
        let genesis = Block { header, transactions: txs, compact: vec![bundle] };
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
        let bundle = self.emission_bundle(miner_address, height, reward);
        self.append_bundle(&bundle).expect("coinbase notes");
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
            notes_root: self.notes.root(),
            tags_root: self.tags.root(),
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
        let block = Block { header, transactions: txs, compact: vec![bundle] };
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
        let bundle = self.emission_bundle(staker_address, height, reward);
        self.append_bundle(&bundle)?;
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
            notes_root: self.notes.root(),
            tags_root: self.tags.root(),
        };
        let block = Block { header, transactions: txs, compact: vec![bundle] };
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

    pub fn rebuild_notes(&mut self) -> Result<(), &'static str> {
        self.notes = NoteCommitmentTree::new();
        self.tags = SpendTagSet::new();
        for block in &self.blocks {
            for bundle in &block.compact {
                if !bundle.verify_conservation() {
                    return Err("compact conservation failed");
                }
                for tag in bundle.spend_tags() {
                    self.tags.insert(tag)?;
                }
                for c in bundle.output_commitments() {
                    self.notes.append(c);
                }
            }
            if block.header.notes_root != self.notes.root() {
                return Err("notes_root mismatch");
            }
            if block.header.tags_root != self.tags.root() {
                return Err("tags_root mismatch");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consensus::pow::{meets_difficulty, sha256d};
    use crate::notes::payout::SealedPayout;

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
        assert_ne!(block.header.notes_root, [0u8; 32]);
        assert!(!block.compact.is_empty());
    }

    #[test]
    fn pos_rejects_below_min_stake() {
        let mut chain = Blockchain::new();
        assert!(chain.mint_pos_block("staker", 1).is_err());
    }

    #[test]
    fn pos_accepts_min_stake() {
        let mut chain = Blockchain::new();
        let block = chain.mint_pos_block("staker", CHAIN_PARAMS.min_stake).expect("pos mint");
        assert_eq!(block.header.block_type, BlockType::PoS);
        assert_eq!(block.header.height, 1);
        assert_ne!(block.header.notes_root, [0u8; 32]);
    }

    #[test]
    fn sealed_payout_is_not_worker_label() {
        let sealed = SealedPayout::from_ticket(b"miner", 1);
        assert_ne!(sealed.dest, [0u8; 32]);
    }

    #[test]
    fn rebuild_notes_matches_live_roots() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("miner");
        let notes = chain.notes.root();
        let tags = chain.tags.root();
        chain.rebuild_notes().expect("rebuild");
        assert_eq!(chain.notes.root(), notes);
        assert_eq!(chain.tags.root(), tags);
    }
}
