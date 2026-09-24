use std::collections::HashMap;
use chrono::Utc;
use crate::chain::block::{Block, BlockHeader, BlockType};
use crate::consensus::pow;
use crate::consensus::difficulty;
use crate::notes::action::ActionBundle;
use crate::notes::launch::LaunchSet;
use crate::notes::payout::SealedPayout;
use crate::notes::spend::emission_bundle;
use crate::notes::stake::StakeProof;
use crate::notes::tags::SpendTagSet;
use crate::params::CHAIN_PARAMS;

pub(crate) fn verify_bundle_against(
    launch: &LaunchSet,
    bundle: &ActionBundle,
    allow_emission: bool,
    reward: Option<u64>,
    height: u64,
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
        if !proof.verify(&cms, reward, height) {
            return Err("emission OR failed");
        }
        return Ok(());
    }
    if bundle.real_spends().is_empty() {
        return Err("empty bundle");
    }
    let mut ctx = Vec::new();
    ctx.extend_from_slice(&launch.window_root());
    ctx.extend_from_slice(&height.to_le_bytes());
    for spend in bundle.real_spends() {
        let proof = spend.proof.as_ref().ok_or("note proof required")?;
        let outs: Vec<_> = bundle
            .real_outputs()
            .into_iter()
            .map(|o| o.value_commitment.clone())
            .collect();
        if outs.is_empty() {
            return Err("spend missing output");
        }
        let mut transcript = Vec::new();
        transcript.extend_from_slice(&spend.spend_tag);
        for o in &outs {
            transcript.extend_from_slice(&o.commitment);
        }
        transcript.extend_from_slice(&bundle.fee_commitment.commitment);
        transcript.extend_from_slice(&launch.window_root());
        transcript.extend_from_slice(&height.to_le_bytes());
        if !proof.verify(
            &spend.spend_tag,
            &spend.rerand,
            &outs,
            &bundle.fee_commitment,
            launch,
            &ctx,
            &transcript,
            spend.pred.id,
            spend.pred.commit,
        ) {
            return Err("note proof failed");
        }
    }
    bundle.verify_programs(height)?;
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
        height: u64,
    ) -> Result<(), &'static str> {
        verify_bundle_against(&self.launch, bundle, allow_emission, reward, height)?;
        for tag in bundle.spend_tags() {
            self.tags.insert(tag)?;
        }
        for n in bundle.output_records() {
            self.launch.append_note(n);
        }
        Ok(())
    }

    pub fn apply_transfer(&mut self, bundle: &ActionBundle) -> Result<(), &'static str> {
        if bundle.is_emission() {
            return Err("use emission path");
        }
        let h = self.height();
        self.append_bundle(bundle, false, None, h)
    }

    fn create_genesis(&mut self) {
        let pay = SealedPayout::from_wallet_seed(b"genesis-wallet", 0);
        let bundle = emission_bundle(&pay.spend, &pay.scan, 0, CHAIN_PARAMS.pow_block_reward, [0u8; 16]);
        self.append_bundle(&bundle, true, Some(CHAIN_PARAMS.pow_block_reward), 0)
            .expect("genesis");
        let compact = vec![bundle];
        let header = BlockHeader {
            version: 4,
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

    pub fn mine_pow_with_payout(
        &mut self,
        _ticket: &str,
        payout: &SealedPayout,
        extra: Vec<ActionBundle>,
    ) -> Block {
        for b in &extra {
            let h = self.height();
            self.append_bundle(b, false, None, h).expect("extra");
        }
        let height = self.height();
        let reward = self.current_pow_reward();
        let bundle = emission_bundle(&payout.spend, &payout.scan, height, reward, [7u8; 16]);
        self.append_bundle(&bundle, true, Some(reward), height).expect("emission");
        let mut compact = extra;
        compact.push(bundle);
        let mut header = BlockHeader {
            version: 4,
            height,
            prev_hash: self.tip_hash(),
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

    pub fn mine_pow_block(&mut self, wallet_seed: &str) -> Block {
        let pay = SealedPayout::from_wallet_seed(wallet_seed.as_bytes(), self.height());
        self.mine_pow_with_payout("local", &pay, Vec::new())
    }

    pub fn mine_pow_with_bundles(&mut self, wallet_seed: &str, extra: Vec<ActionBundle>) -> Block {
        let pay = SealedPayout::from_wallet_seed(wallet_seed.as_bytes(), self.height());
        self.mine_pow_with_payout("local", &pay, extra)
    }

    pub fn mint_pos_block(&mut self, payout: &SealedPayout, proof: &StakeProof) -> Result<Block, &'static str> {
        let reward = proof.reward();
        if reward == 0 {
            return Err("zero pos reward");
        }
        let height = self.height();
        let bundle = emission_bundle(&payout.spend, &payout.scan, height, reward, [9u8; 16]);
        self.append_bundle(&bundle, true, Some(reward), height)?;
        let compact = vec![bundle];
        let header = BlockHeader {
            version: 4,
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
            } else if let Some(b) = block.compact.iter().find(|b| b.is_emission()) {
                b.emission.as_ref().map(|e| e.reward)
            } else {
                None
            };
            for bundle in &block.compact {
                let allow = bundle.is_emission();
                let r = if allow { reward } else { None };
                verify_bundle_against(&self.launch, bundle, allow, r, block.header.height)?;
                for tag in bundle.spend_tags() {
                    self.tags.insert(tag)?;
                }
                for n in bundle.output_records() {
                    self.launch.append_note(n);
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
    use crate::notes::keys::SpendKey;
    use crate::notes::spend::transfer_window_bundle;
    use crate::notes::stake::StakeProof;

    #[test]
    fn compact_only_pow() {
        let mut chain = Blockchain::new();
        let block = chain.mine_pow_block("local-miner");
        assert!(block.compact.len() >= 1);
        assert_eq!(block.header.notes_root, chain.launch.commitment());
    }

    #[test]
    fn ticket_is_not_payout_seed() {
        let a = SealedPayout::from_wallet_seed(b"local-miner", 1);
        let b = SealedPayout::from_wallet_seed(b"pool-ticket", 1);
        assert_ne!(a.dest.bytes, b.dest.bytes);
    }

    #[test]
    fn spend_uses_key_image_not_dest() {
        let mut chain = Blockchain::new();
        let pay = SealedPayout::from_wallet_seed(b"independent-dev-wallet", 1);
        let _ = chain.mine_pow_with_payout("pool-ticket", &pay, Vec::new());
        let reward = CHAIN_PARAMS.pow_block_reward;
        let r = blinding_from_seed(&[b"emit-r".as_ref(), &pay.spend.sk.to_bytes(), &1u64.to_le_bytes()].concat());
        let cm = crate::notes::proof::commit_with_asset(reward, &r, &[0u8; 32]).commitment;
        let out = SpendKey::from_wallet_seed(b"recv");
        let r_out = blinding_from_seed(b"o");
        let r_fee = r - r_out;
        let bundle = transfer_window_bundle(
            &pay.spend, &pay.scan, &chain.launch, cm, reward, &r, &out,
            reward - 1, &r_out, 1, &r_fee, [4u8; 16], 2,
        ).expect("owned spend");
        let encoded = format!("{:?}", bundle.real_spends()[0].proof);
        assert!(!encoded.contains("dest"));
        assert!(!encoded.contains("ring:"));
        let _ = chain.mine_pow_with_payout("pool-ticket", &pay, vec![bundle.clone()]);
        assert!(chain.apply_transfer(&bundle).is_err());
    }

    #[test]
    fn pos_takes_stake_proof() {
        let mut chain = Blockchain::new();
        let _ = chain.mine_pow_block("staker-local");
        let r = blinding_from_seed(b"stk");
        let coins = CHAIN_PARAMS.min_stake;
        let c = ValueCommitment::commit(coins, &r, &PedersenGenerators::default());
        chain.launch.append(c.commitment);
        let proof = StakeProof::create(&chain.launch, &c, coins, CHAIN_PARAMS.pos_coin_age_min + 3600)
            .expect("stake");
        let pay = SealedPayout::from_wallet_seed(b"staker-wallet", chain.height());
        assert!(chain.mint_pos_block(&pay, &proof).is_ok());
    }
}
