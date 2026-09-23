//! Stake is a committed note. Coins never appear on the minted block API.

use super::commitment::{PedersenGenerators, ValueCommitment};
use super::launch::LaunchSet;
use crate::consensus::pos::{pos_reward, validate_stake};
use curve25519_dalek::scalar::Scalar;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StakeProof {
    pub stake_commitment: ValueCommitment,
    pub seconds_held: u64,
    #[serde(skip)]
    coins: u64,
}

impl StakeProof {
    pub fn create(
        launch: &LaunchSet,
        coins: u64,
        blinding: &Scalar,
        seconds_held: u64,
    ) -> Result<Self, &'static str> {
        validate_stake(coins, seconds_held)?;
        let gens = PedersenGenerators::default();
        let stake_commitment = ValueCommitment::commit(coins, blinding, &gens);
        if !launch.contains(stake_commitment.commitment) {
            return Err("stake note not in living set");
        }
        Ok(Self {
            stake_commitment,
            seconds_held,
            coins,
        })
    }

    pub fn reward(&self) -> u64 {
        pos_reward(self.coins, self.seconds_held)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::commitment::blinding_from_seed;
    use crate::notes::launch::LaunchSet;
    use crate::params::CHAIN_PARAMS;
    use serde_json;

    #[test]
    fn serialized_stake_omits_coins() {
        let r = blinding_from_seed(b"stake");
        let gens = PedersenGenerators::default();
        let coins = CHAIN_PARAMS.min_stake;
        let c = ValueCommitment::commit(coins, &r, &gens);
        let mut set = LaunchSet::standard();
        set.append(c.commitment);
        let p = StakeProof::create(&set, coins, &r, CHAIN_PARAMS.pos_coin_age_min + 3600)
            .expect("ok");
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains(&coins.to_string()));
        assert_eq!(p.reward(), pos_reward(coins, CHAIN_PARAMS.pos_coin_age_min + 3600));
    }
}
