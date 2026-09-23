//! Stake is a living-set note. Coins are not serialized.

use super::commitment::ValueCommitment;
use super::launch::LaunchSet;
use crate::consensus::pos::{pos_reward, validate_stake};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StakeProof {
    pub note_id: [u8; 32],
    pub stake_commitment: ValueCommitment,
    pub seconds_held: u64,
    #[serde(skip)]
    coins: u64,
}

impl StakeProof {
    pub fn create(
        launch: &LaunchSet,
        stake_commitment: &ValueCommitment,
        coins: u64,
        seconds_held: u64,
    ) -> Result<Self, &'static str> {
        validate_stake(coins, seconds_held)?;
        let note_id = stake_commitment.commitment;
        if !launch.contains(note_id) {
            return Err("stake note not in living window");
        }
        Ok(Self {
            note_id,
            stake_commitment: stake_commitment.clone(),
            seconds_held,
            coins,
        })
    }

    pub fn reward(&self) -> u64 {
        pos_reward(self.coins, self.seconds_held)
    }
}
