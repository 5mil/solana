use std::collections::HashMap;
use chrono::Utc;
use crate::chain::block::{Block, BlockHeader, BlockType};
use crate::consensus::pow;
use crate::consensus::difficulty;
use crate::notes::action::ActionBundle;
use crate::notes::launch::LaunchSet;
use crate::notes::payout::SealedPayout;
use crate::notes::spend::{emission_bundle, transfer_window_bundle};
use crate::notes::stake::StakeProof;
use crate::notes::tags::SpendTagSet;
use crate::params::CHAIN_PARAMS;

pub(crate) fn verify_bundle_against(
    launch: &LaunchSet,
    bundle: &ActionBundle,
    allow_emission: bool,
    reward: Option<u64>,
) -> Result<(), &'static str> {
    if !bundle.verify_conservation() {
        return Err("conservation failed");
    }
    if bundle.is_emission() {
        if !allow_emission {
            return Err("emission only on the block emission path");
        }
        let reward = reward.ok_or("emission reward required")?;
        let cms: Vec<[u8; 32]> = bundle
            .actions
            .iter()
            .filter_map(|a| a.output.as_ref())
            .map(|o| o.value_commitment.commitment)
            .collect();
        let proof = bundle.emission.as_ref().ok_or("emission OR required")?;
        if !proof.verify(&cms, reward) {
            return Err("emission OR failed");
        }
        return Ok(());
    }
    if bundle.real_spends().is_empty() {
        return Err("empty bundle");
    }
    let live = launch.window_root();
    for spend in bundle.real_spends() {
        let proof = spend.proof.as_ref().ok_or("note proof required")?;
        if !proof.membership.verify(live) {
            return Err("window membership failed");
        }
        if !launch.contains(proof.membership.note_id) {
            return Err("note not in living set");
        }
        if !proof.auth.verify(&spend.spend_tag, &live, &spend.rerand.commitment) {
            return Err("spend auth failed");
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
    pub block_index: HashMap<[u8; 32], usize>,
    pub current_difficulty: u32,
    pub total_supply: u64,
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
            launch: LaunchSet::standard(),
            tags: SpendTagSet::new(),
        };
        chain.create_genesis();
        chain
    }

    fn append_bundle(
        &mut self,
        bundle: &ActionBundle,
        allow_emission: bool,
        reward: Option<u64>,
    ) -> Result<(), &'static str> {
        verify_bundle_against(&self.launch, bundle, allow_emission, reward)?;
        for tag in bundle.spend_tags() {
            self.tags.insert(tag)?;
        }
        for id in bundle.output_note_ids() {
            self.launch.append(id);
        }
        Ok(())
    }

    pub fn apply_transfer(&mut self, bundle: &ActionBundle) -> Result<(), &'static str> {
        if bundle.is_emission() {
            return Err("use emission path");
        }
        self.append_bundle(bundle, false, None)
    }

    fn create_genesis(&mut self) {
        let pay = SealedPayout::from_wallet_seed(b"genesis-wallet", 0);
        let bundle = emission_bundle(&pay.spend, &pay.scan, 0, CHAIN_PARAMS.pow_block_reward, [0u8; 16]);
        self.append_bundle(&bundle, true, Some(CHAIN_PARAMS.pow_block_reward))
            .expect("genesis");
        let compact = vec![bundle];
        let header = BlockHeader {
            version: 3,
            height: 0,
            prev_hash: [0u8; 32],
            merkle_root: Block::compute_merkle_root(&compact),
            timestamp: CHAIN_PARAMS.genesis_timestamp,
            difficulty: self.current_difficulty,
            block_type: BlockType::PoW,
            nonce: 0,
            stake_modifier: [0u8; 32],
            notes_root: self.launch.commitment(),
            tags_root: self.tags.root(),
        };
        let genesis = Block { header, compact };
        let hash = genesis.hash();
        self.block_index.insert(hash, 0);
        self.total_supply += CHAIN_PARAMS.pow_block_reward;
        self.blocks.push(genesis);
    }

    pub fn tip_hash(&self) -> [u8; 32] {
        self.blocks.last().map(|b| b.hash()).unwrap_or([0u8; 32])
    }

    pub fn height(&self) -> u64 {
        self.blocks.len() as u64
    }

    pub fn mine_pow_block(&mut self, wallet_seed: &str) -> Block {
        self.mine_pow_with_bundles(wallet_seed, Vec::new())
    }

    pub fn mine_pow_with_bundles(&mut self, wallet_seed: &str, extra: Vec<ActionBundle>) -> Block {
        for b in &extra {
            self.append_bundle(b, false, None).expect("extra");
        }
        let prev_hash = self.tip_hash();
        let height = self.height();
        let reward = self.current_pow_reward();
        let pay = SealedPayout::from_wallet_seed(wallet_seed.as_bytes(), height);
        let bundle = emission_bundle(&pay.spend, &pay.scan, height, reward, height.to_le_bytes()[..16].try_into().unwrap_or([0u8; 16]));
        self.append_bundle(&bundle, true, Some(reward)).expect("emission");
        let mut compact = extra;
        compact.push(bundle);
        let mut header = BlockHeader {
            version: 3,
            height,
            prev_hash,
            merkle_root: Block::compute_merkle_root(&compact),
            timestamp: Utc::now().timestamp(),
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
        let block = Block { header, compact };
        let block_hash = block.hash();
        self.block_index.insert(block_hash, self.blocks.len());
        self.total_supply += reward;
        self.blocks.push(block.clone());
        self.maybe_retarget();
        block
    }

    pub fn mint_pos_block(&mut self, wallet_seed: &str, proof: &StakeProof) -> Result<Block, &'static str> {
        let reward = proof.reward();
        if reward == 0 {
            return Err("zero pos reward");
        }
        let height = self.height();
        let pay = SealedPayout::from_wallet_seed(wallet_seed.as_bytes(), height);
        let bundle = emission_bundle(&pay.spend, &pay.scan, height, reward, [9u8; 16]);
        self.append_bundle(&bundle, true, Some(reward))?;
        let compact = vec![bundle];
        let header = BlockHeader {
            version: 3,
            height,
            prev_hash: self.tip_hash(),
            merkle_root: Block::compute_merkle_root(&compact),
            timestamp: Utc::now().timestamp(),
            difficulty: self.current_difficulty,
            block_type: BlockType::PoS,
            nonce: 0,
            stake_modifier: [0u8; 32],
            notes_root: self.launch.commitment(),
            tags_root: self.tags.root(),
        };
        let block = Block { header, compact };
        let block_hash = block.hash();
        self.block_index.insert(block_hash, self.blocks.len());
        self.total_supply += reward;
        self.blocks.push(block.clone());
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
        self.launch = LaunchSet::standard();
        self.tags = SpendTagSet::new();
        for block in &self.blocks {
            let reward = if block.header.block_type == BlockType::PoW {
                Some(pow_reward_at_height(block.header.height))
            } else {
                None
            };
            for bundle in &block.compact {
                let allow = bundle.is_emission();
                let r = if allow { reward } else { None };
                verify_bundle_against(&self.launch, bundle, allow, r)?;
                for tag in bundle.spend_tags() {
                    self.tags.insert(tag)?;
                }
                for id in bundle.output_note_ids() {
                    self.launch.append(id);
                }
            }
            if block.header.notes_root != self.launch.commitment() {
                return Err("notes_root mismatch");
            }
        }
        Ok(())
    }
}

fn pow_reward_at_height(height: u64) -> u64 {
    let halvings = height / 210_000;
    if halvings >= 64 { 0 } else { CHAIN_PARAMS.pow_block_reward >> halvings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::commitment::{blinding_from_seed, PedersenGenerators, ValueCommitment};
    use crate::notes::keys::{note_id, SpendKey};
    use crate::notes::proof::asset_scalar;
    use crate::notes::stake::StakeProof;

    #[test]
    fn compact_only_pow() {
        let mut chain = Blockchain::new();
        let block = chain.mine_pow_block("wallet-miner");
        assert!(block.compact.len() >= 1);
        assert_eq!(block.header.notes_root, chain.launch.commitment());
    }

    #[test]
    fn spend_needs_owner_key() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("wallet-miner");
        let pay = SealedPayout::from_wallet_seed(b"wallet-miner", 1);
        let reward = CHAIN_PARAMS.pow_block_reward;
        let r = blinding_from_seed(&[b"emit-r".as_ref(), &pay.spend.sk.to_bytes(), &1u64.to_le_bytes()].concat());
        let cm = crate::notes::proof::commit_with_asset(reward, &r, &[0u8; 32]).commitment;
        let out = SpendKey::from_wallet_seed(b"recv");
        let r_out = blinding_from_seed(b"o");
        let r_fee = r - r_out;
        let bundle = transfer_window_bundle(
            &pay.spend,
            &pay.scan,
            &chain.launch,
            cm,
            reward,
            &r,
            &out,
            reward - 1,
            &r_out,
            1,
            &r_fee,
            [4u8; 16],
        )
        .expect("owned spend");
        assert!(bundle.real_spends()[0].proof.is_some());
        let _ = chain.mine_pow_with_bundles("wallet-2", vec![bundle.clone()]);
        assert!(chain.apply_transfer(&bundle).is_err());
    }

    #[test]
    fn pos_takes_stake_proof() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("w");
        let sk = SpendKey::from_wallet_seed(b"staker-w");
        let r = blinding_from_seed(b"stk");
        let coins = CHAIN_PARAMS.min_stake;
        let c = ValueCommitment::commit(coins, &r, &PedersenGenerators::default());
        let nid = note_id(&c.commitment, &sk.pk(), &asset_scalar(&[0u8; 32]));
        chain.launch.append(nid);
        let proof = StakeProof::create(&chain.launch, &c, coins, CHAIN_PARAMS.pos_coin_age_min + 3600)
            .expect("stake");
        assert!(chain.mint_pos_block("staker-w", &proof).is_ok());
    }
}
