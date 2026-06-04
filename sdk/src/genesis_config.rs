//! Genesis configuration — extended for hybrid PoW/PoS consensus.
//! The original PoH-specific fields have been replaced with consensus_config.

use {
    crate::{
        clock::{DEFAULT_TICKS_PER_SLOT, DEFAULT_SLOTS_PER_EPOCH},
        consensus::{HybridConsensusConfig, PowConfig},
        epoch_schedule::EpochSchedule,
        fee_calculator::FeeRateGovernor,
        inflation::Inflation,
        pubkey::Pubkey,
        rent::Rent,
        shred_version::version_from_hash,
    },
    bincode::{deserialize, serialize},
    itertools::Itertools,
    serde::{Deserialize, Serialize},
    std::{
        collections::BTreeMap,
        fmt,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    },
};

/// Cluster type, used to distinguish mainnet, testnet, devnet, and local.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ClusterType {
    Testnet,
    MainnetBeta,
    Devnet,
    Development,
}

impl Default for ClusterType {
    fn default() -> Self { ClusterType::Development }
}

/// Genesis configuration block.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GenesisConfig {
    /// Unix timestamp at genesis.
    pub creation_time: i64,
    /// Pre-allocated accounts (pubkey -> account).
    pub accounts: BTreeMap<Pubkey, crate::account::AccountSharedData>,
    /// Native program names and their pubkeys.
    pub native_instruction_processors: Vec<(String, Pubkey)>,
    /// Accounts to reward at genesis.
    pub rewards_pools: BTreeMap<Pubkey, crate::account::AccountSharedData>,
    /// Ticks per slot (retained for internal scheduler use).
    pub ticks_per_slot: u64,
    /// Epoch schedule.
    pub epoch_schedule: EpochSchedule,
    /// Fee rate governor.
    pub fee_rate_governor: FeeRateGovernor,
    /// Rent config.
    pub rent: Rent,
    /// Inflation curve.
    pub inflation: Inflation,
    /// Cluster type.
    pub cluster_type: ClusterType,
    /// Hybrid PoW/PoS consensus parameters (replaces PoH config).
    pub consensus_config: HybridConsensusConfig,
}

impl GenesisConfig {
    pub fn new(
        accounts: impl IntoIterator<Item = (Pubkey, crate::account::AccountSharedData)>,
        native_instruction_processors: Vec<(String, Pubkey)>,
    ) -> Self {
        Self {
            accounts: accounts.into_iter().collect(),
            native_instruction_processors,
            ticks_per_slot: DEFAULT_TICKS_PER_SLOT,
            creation_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            ..Self::default()
        }
    }

    /// Compute the coinbase block reward at a given block height.
    pub fn pow_block_reward(&self, height: u64) -> u64 {
        self.consensus_config.pow.block_reward_at_height(height)
    }

    pub fn hash(&self) -> solana_sdk::hash::Hash {
        let serialized = serialize(self).unwrap_or_default();
        solana_sdk::hash::hash(&serialized)
    }

    pub fn shred_version(&self) -> u16 {
        version_from_hash(&self.hash())
    }

    pub fn write_to_file(&self, ledger_path: &Path) -> std::io::Result<()> {
        let serialized = serialize(self).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })?;
        let path = ledger_path.join("genesis.bin");
        std::fs::write(path, serialized)
    }

    pub fn load(ledger_path: &Path) -> std::io::Result<Self> {
        let path = ledger_path.join("genesis.bin");
        let data = std::fs::read(path)?;
        deserialize(&data).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
        })
    }
}

impl fmt::Display for GenesisConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "GenesisConfig {{ creation_time: {}, consensus: {}/{}, accounts: {}, ticks_per_slot: {} }}",
            self.creation_time,
            self.consensus_config.coin_name,
            self.consensus_config.ticker,
            self.accounts.len(),
            self.ticks_per_slot,
        )
    }
}
