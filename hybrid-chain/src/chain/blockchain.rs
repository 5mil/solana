use std::collections::HashMap;
use chrono::Utc;
use crate::chain::block::{Block, BlockHeader, BlockType};
use crate::chain::transaction::Transaction;
use crate::consensus::pow;
use crate::consensus::pos;
use crate::consensus::difficulty;
use crate::notes::action::{coinbase_bundle, emission_blinding, emission_commitment, ActionBundle};
use crate::notes::auth::RangeProof;
use crate::notes::commitment::ValueCommitment;
use crate::notes::launch::LaunchSet;
use crate::notes::payout::SealedPayout;
use crate::notes::stake::StakeProof;
use crate::notes::tags::SpendTagSet;
use crate::notes::tree::NoteCommitmentTree;
use crate::params::CHAIN_PARAMS;

pub(crate) fn verify_bundle_against(
    launch: &LaunchSet,
    bundle: &ActionBundle,
    allow_emission: bool,
) -> Result<(), &'static str> {
    if !bundle.verify_conservation() {
        return Err("conservation failed");
    }
    if bundle.is_emission() {
        if !allow_emission {
            return Err("emission only on the block emission path");
        }
        for out in bundle.real_outputs() {
            if let Some(r) = &out.range {
                if !r.verify(&out.value_commitment) {
                    return Err("emission range failed");
                }
            } else {
                return Err("emission range required");
            }
        }
        return Ok(());
    }
    if bundle.real_spends().is_empty() {
        return Err("empty bundle");
    }
    for spend in bundle.real_spends() {
        if spend.ring.is_empty() || spend.link.is_none() {
            return Err("ring link required");
        }
        for cm in &spend.ring {
            if !launch.contains(*cm) {
                return Err("ring member not in living set");
            }
        }
        let link = spend.link.as_ref().unwrap();
        if !link.verify(&spend.ring, &spend.rerand) {
            return Err("link failed");
        }
        if let Some(r) = &spend.range {
            if !r.verify(&spend.rerand) {
                return Err("spend range failed");
            }
        } else {
            return Err("spend range required");
        }
    }
    for out in bundle.real_outputs() {
        if let Some(r) = &out.range {
            if !r.verify(&out.value_commitment) {
                return Err("output range failed");
            }
        } else {
            return Err("output range required");
        }
    }
    if let Some(r) = &bundle.fee_range {
        if !r.verify(&bundle.fee_commitment) {
            return Err("fee range failed");
        }
    } else {
        return Err("fee range required");
    }
    Ok(())
}

fn emission_matches(bundle: &ActionBundle, height: u64, reward: u64) -> bool {
    bundle.real_outputs().iter().any(|o| {
        o.value_commitment.commitment
            == emission_commitment(&o.one_time_dest, height, reward).commitment
    })
}

#[derive(Debug)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub block_index: HashMap<[u8; 32], usize>,
    pub current_difficulty: u32,
    pub total_supply: u64,
    pub notes: NoteCommitmentTree,
    pub launch: LaunchSet,
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
            launch: LaunchSet::standard(),
            tags: SpendTagSet::new(),
        };
        chain.create_genesis();
        chain
    }

    fn append_bundle(&mut self, bundle: &ActionBundle, allow_emission: bool) -> Result<(), &'static str> {
        verify_bundle_against(&self.launch, bundle, allow_emission)?;
        for tag in bundle.spend_tags() {
            self.tags.insert(tag)?;
        }
        for c in bundle.output_commitments() {
            self.notes.append(c);
            self.launch.append(c);
        }
        Ok(())
    }

    pub fn apply_transfer(&mut self, bundle: &ActionBundle) -> Result<(), &'static str> {
        if bundle.is_emission() {
            return Err("use emission path for coinbase");
        }
        self.append_bundle(bundle, false)
    }

    fn emission_bundle(&self, ticket: &str, height: u64, reward: u64) -> ActionBundle {
        let sealed = SealedPayout::from_ticket(ticket.as_bytes(), height);
        let commit = emission_commitment(&sealed.dest, height, reward);
        let range = RangeProof::prove(reward, &emission_blinding(&sealed.dest, height));
        coinbase_bundle(sealed.dest, &sealed.scan_seed, commit, [0u8; 32], range)
    }

    fn create_genesis(&mut self) {
        let genesis_sealed = SealedPayout::from_ticket(b"genesis", 0);
        let genesis_tx = Transaction::coinbase(0, CHAIN_PARAMS.pow_block_reward, &genesis_sealed.dest);
        let txs = vec![genesis_tx];
        let merkle = Block::compute_merkle_root(&txs);
        let bundle = self.emission_bundle("genesis", 0, CHAIN_PARAMS.pow_block_reward);
        self.append_bundle(&bundle, true).expect("genesis notes");
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
            notes_root: self.launch.commitment(),
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
        self.mine_pow_with_bundles(miner_address, Vec::new())
    }

    pub fn mine_pow_with_bundles(&mut self, miner_address: &str, extra: Vec<ActionBundle>) -> Block {
        for b in &extra {
            self.append_bundle(b, false).expect("extra compact bundle");
        }
        let prev_hash = self.tip_hash();
        let height = self.height();
        let reward = self.current_pow_reward();
        let sealed = SealedPayout::from_ticket(miner_address.as_bytes(), height);
        let coinbase = Transaction::coinbase(height, reward, &sealed.dest);
        let txs = vec![coinbase];
        let merkle = Block::compute_merkle_root(&txs);
        let now = Utc::now().timestamp();
        let bundle = self.emission_bundle(miner_address, height, reward);
        assert!(emission_matches(&bundle, height, reward));
        self.append_bundle(&bundle, true).expect("coinbase notes");
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
            notes_root: self.launch.commitment(),
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
        let mut compact = extra;
        compact.push(bundle);
        let block = Block { header, transactions: txs, compact };
        let block_hash = block.hash();
        self.block_index.insert(block_hash, self.blocks.len());
        self.total_supply += reward;
        self.blocks.push(block.clone());
        self.maybe_retarget();
        log::info!("PoW block #{} mined: {} (nonce={})", height, hex::encode(block_hash), nonce);
        block
    }

    pub fn mint_pos_block(&mut self, staker_address: &str, proof: &StakeProof) -> Result<Block, &'static str> {
        let reward = proof.reward();
        if reward == 0 {
            return Err("zero pos reward");
        }
        let prev_hash = self.tip_hash();
        let height = self.height();
        let staker_sealed = SealedPayout::from_ticket(staker_address.as_bytes(), height);
        let coinstake = Transaction::coinstake(0, reward, &staker_sealed.dest);
        let txs = vec![coinstake];
        let merkle = Block::compute_merkle_root(&txs);
        let now = Utc::now().timestamp();
        let bundle = self.emission_bundle(staker_address, height, reward);
        self.append_bundle(&bundle, true)?;
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
            notes_root: self.launch.commitment(),
            tags_root: self.tags.root(),
        };
        let block = Block { header, transactions: txs, compact: vec![bundle] };
        let block_hash = block.hash();
        self.block_index.insert(block_hash, self.blocks.len());
        self.total_supply += reward;
        self.blocks.push(block.clone());
        log::info!("PoS block #{} minted: {} (reward hidden in commitment)", height, hex::encode(block_hash));
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
        }
    }

    pub fn rebuild_notes(&mut self) -> Result<(), &'static str> {
        self.notes = NoteCommitmentTree::new();
        self.launch = LaunchSet::standard();
        self.tags = SpendTagSet::new();
        for block in &self.blocks {
            for bundle in &block.compact {
                let allow = bundle.is_emission();
                verify_bundle_against(&self.launch, bundle, allow)?;
                for tag in bundle.spend_tags() {
                    self.tags.insert(tag)?;
                }
                for c in bundle.output_commitments() {
                    self.notes.append(c);
                    self.launch.append(c);
                }
            }
            if block.header.notes_root != self.launch.commitment() {
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
    use crate::notes::commitment::blinding_from_seed;
    use crate::notes::payout::SealedPayout;
    use crate::notes::stake::StakeProof;

    #[test]
    fn genesis_and_pow_bind_live_root() {
        let mut chain = Blockchain::new();
        let block = chain.mine_pow_block("miner");
        assert_eq!(block.header.height, 1);
        assert_eq!(block.header.notes_root, chain.launch.commitment());
        assert_eq!(block.transactions[0].outputs[0].value, 0);
        let bytes = bincode::serialize(&block.header).unwrap();
        assert!(meets_difficulty(&sha256d(&bytes), block.header.difficulty));
    }

    #[test]
    fn pos_uses_stake_proof_not_naked_coins() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("miner");
        let r = blinding_from_seed(b"stake-open");
        let coins = CHAIN_PARAMS.min_stake;
        let c = ValueCommitment::commit(coins, &r, &crate::notes::commitment::PedersenGenerators::default());
        chain.launch.append(c.commitment);
        let proof = StakeProof::create(
            &chain.launch,
            coins,
            &r,
            CHAIN_PARAMS.pos_coin_age_min + 3600,
        )
        .expect("stake");
        let block = chain.mint_pos_block("staker", &proof).expect("pos");
        assert_eq!(block.header.block_type, BlockType::PoS);
        assert_eq!(block.transactions[0].outputs[1].value, 0);
    }

    #[test]
    fn rebuild_matches_live_root() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("miner");
        let live = chain.launch.commitment();
        chain.rebuild_notes().expect("rebuild");
        assert_eq!(chain.launch.commitment(), live);
    }

    #[test]
    fn hidden_spend_does_not_replay_note_cm() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("miner");
        let height = 1u64;
        let sealed = SealedPayout::from_ticket(b"miner", height);
        let reward = CHAIN_PARAMS.pow_block_reward;
        let in_blind = emission_blinding(&sealed.dest, height);
        let note = emission_commitment(&sealed.dest, height, reward);
        let r_out = blinding_from_seed(b"wout");
        let r_fee = in_blind - r_out;
        let bundle = crate::notes::transfer_window_bundle(
            &sealed.spend_secret(),
            &chain.launch,
            note.commitment,
            reward,
            &in_blind,
            [7u8; 32],
            b"recv-scan",
            reward - 1,
            &r_out,
            1,
            &r_fee,
            [0u8; 32],
        )
        .expect("window transfer");
        assert_ne!(bundle.real_spends()[0].rerand.commitment, note.commitment);
        let block = chain.mine_pow_with_bundles("miner2", vec![bundle.clone()]);
        assert!(block.compact.len() >= 2);
        assert!(chain.apply_transfer(&bundle).is_err());
    }

    #[test]
    fn extra_emission_rejected() {
        let mut chain = Blockchain::new();
        let extra = chain.emission_bundle("sneak", 1, CHAIN_PARAMS.pow_block_reward);
        assert_eq!(chain.apply_transfer(&extra), Err("use emission path for coinbase"));
    }
}
