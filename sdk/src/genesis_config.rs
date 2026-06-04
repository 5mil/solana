//! The chain's genesis config.
//! Modified for SHA256d PoW/PoS hybrid consensus.
//! `poh_config` has been replaced by `consensus_config: HybridConsensusConfig`.

#![cfg(feature = "full")]

use {
    crate::{
        account::{Account, AccountSharedData},
        clock::UnixTimestamp,
        consensus::{
            HybridConsensusConfig, PowConfig, PosConfig, ConsensusType,
        },
        epoch_schedule::EpochSchedule,
        fee_calculator::FeeRateGovernor,
        hash::{hash, Hash},
        inflation::Inflation,
        native_token::lamports_to_sol,
        poh_config::PohConfig,
        pubkey::Pubkey,
        rent::Rent,
        shred_version::compute_shred_version,
        signature::{Keypair, Signer},
        system_program,
    },
    bincode::{deserialize, serialize},
    chrono::{TimeZone, Utc},
    memmap2::Mmap,
    std::{
        collections::BTreeMap,
        fmt,
        fs::{File, OpenOptions},
        io::Write,
        path::{Path, PathBuf},
        str::FromStr,
        time::{SystemTime, UNIX_EPOCH},
    },
};

pub const DEFAULT_GENESIS_FILE: &str = "genesis.bin";
pub const DEFAULT_GENESIS_ARCHIVE: &str = "genesis.tar.bz2";
pub const DEFAULT_GENESIS_DOWNLOAD_PATH: &str = "/genesis.tar.bz2";

// Retained for ABI compatibility
pub const UNUSED_DEFAULT: u64 = 1024;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, AbiEnumVisitor, AbiExample)]
pub enum ClusterType {
    Testnet,
    MainnetBeta,
    Devnet,
    Development,
}

impl ClusterType {
    pub const STRINGS: [&'static str; 4] = ["development", "devnet", "testnet", "mainnet-beta"];

    pub fn get_genesis_hash(&self) -> Option<Hash> {
        match self {
            // These hashes are now for the hybrid chain genesis blocks.
            // Original Solana network hashes removed — this is a new chain.
            Self::MainnetBeta => None,
            Self::Testnet => None,
            Self::Devnet => None,
            Self::Development => None,
        }
    }
}

impl FromStr for ClusterType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "development" => Ok(ClusterType::Development),
            "devnet" => Ok(ClusterType::Devnet),
            "testnet" => Ok(ClusterType::Testnet),
            "mainnet-beta" => Ok(ClusterType::MainnetBeta),
            _ => Err(format!("{s} is unrecognized for cluster type")),
        }
    }
}

/// Genesis configuration for the hybrid PoW/PoS chain.
/// The `poh_config` field is retained as a deprecated stub for ABI
/// compatibility; all real consensus parameters live in `consensus_config`.
#[derive(Serialize, Deserialize, Debug, Clone, AbiExample, PartialEq)]
pub struct GenesisConfig {
    /// Unix timestamp when the network was bootstrapped.
    pub creation_time: UnixTimestamp,
    /// Pre-funded accounts at genesis (founder allocations, faucet, etc.).
    pub accounts: BTreeMap<Pubkey, Account>,
    /// Built-in program names and their program IDs.
    pub native_instruction_processors: Vec<(String, Pubkey)>,
    /// Reward pool accounts (do not count toward circulating supply).
    pub rewards_pools: BTreeMap<Pubkey, Account>,
    /// DEPRECATED: slots/ticks retained for ABI layout compatibility.
    pub ticks_per_slot: u64,
    /// DEPRECATED: retained for ABI layout compatibility.
    pub unused: u64,
    /// DEPRECATED: PoH config stub — consensus now driven by `consensus_config`.
    pub poh_config: PohConfig,
    /// Retained for binary layout compatibility with v0.23.
    pub __backwards_compat_with_v0_23: u64,
    /// Transaction fee configuration.
    pub fee_rate_governor: FeeRateGovernor,
    /// Rent configuration.
    pub rent: Rent,
    /// Inflation configuration (PoS supplement).
    pub inflation: Inflation,
    /// Epoch schedule (retained for validator/RPC compatibility).
    pub epoch_schedule: EpochSchedule,
    /// Network cluster identifier.
    pub cluster_type: ClusterType,
    /// *** NEW: Hybrid SHA256d PoW/PoS consensus parameters ***
    /// This is the authoritative consensus configuration for the chain.
    pub consensus_config: HybridConsensusConfig,
}

/// Create a minimal genesis config for testing.
pub fn create_genesis_config(lamports: u64) -> (GenesisConfig, Keypair) {
    let faucet_keypair = Keypair::new();
    (
        GenesisConfig::new(
            &[(
                faucet_keypair.pubkey(),
                AccountSharedData::new(lamports, 0, &system_program::id()),
            )],
            &[],
        ),
        faucet_keypair,
    )
}

impl Default for GenesisConfig {
    fn default() -> Self {
        Self {
            creation_time: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as UnixTimestamp,
            accounts: BTreeMap::default(),
            native_instruction_processors: Vec::default(),
            rewards_pools: BTreeMap::default(),
            ticks_per_slot: 1,        // 1 tick per slot in PoW/PoS (not meaningful)
            unused: UNUSED_DEFAULT,
            poh_config: PohConfig::default(),  // deprecated stub
            inflation: Inflation::default(),
            __backwards_compat_with_v0_23: 0,
            fee_rate_governor: FeeRateGovernor::default(),
            rent: Rent::default(),
            epoch_schedule: EpochSchedule::default(),
            cluster_type: ClusterType::Development,
            consensus_config: HybridConsensusConfig::default(),
        }
    }
}

impl GenesisConfig {
    pub fn new(
        accounts: &[(Pubkey, AccountSharedData)],
        native_instruction_processors: &[(String, Pubkey)],
    ) -> Self {
        Self {
            accounts: accounts
                .iter()
                .cloned()
                .map(|(key, account)| (key, Account::from(account)))
                .collect::<BTreeMap<Pubkey, Account>>(),
            native_instruction_processors: native_instruction_processors.to_vec(),
            ..GenesisConfig::default()
        }
    }

    /// Canonical genesis hash (SHA256d of serialized config).
    pub fn hash(&self) -> Hash {
        let serialized = serialize(&self).unwrap();
        hash(&serialized)
    }

    fn genesis_filename(ledger_path: &Path) -> PathBuf {
        Path::new(ledger_path).join(DEFAULT_GENESIS_FILE)
    }

    pub fn load(ledger_path: &Path) -> Result<Self, std::io::Error> {
        let filename = Self::genesis_filename(ledger_path);
        let file = OpenOptions::new()
            .read(true)
            .open(&filename)
            .map_err(|err| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Unable to open {filename:?}: {err:?}"),
                )
            })?;
        let mem = unsafe { Mmap::map(&file) }.map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Unable to map {filename:?}: {err:?}"),
            )
        })?;
        let genesis_config = deserialize(&mem).map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Unable to deserialize {filename:?}: {err:?}"),
            )
        })?;
        Ok(genesis_config)
    }

    pub fn write(&self, ledger_path: &Path) -> Result<(), std::io::Error> {
        let serialized = serialize(&self).map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Unable to serialize: {err:?}"),
            )
        })?;
        std::fs::create_dir_all(ledger_path)?;
        let mut file = File::create(Self::genesis_filename(ledger_path))?;
        file.write_all(&serialized)
    }

    pub fn add_account(&mut self, pubkey: Pubkey, account: AccountSharedData) {
        self.accounts.insert(pubkey, Account::from(account));
    }

    pub fn add_native_instruction_processor(&mut self, name: String, program_id: Pubkey) {
        self.native_instruction_processors.push((name, program_id));
    }

    // -----------------------------------------------------------------
    // Hybrid consensus accessors
    // -----------------------------------------------------------------

    /// PoW block reward at a given block height (applies halving schedule).
    pub fn pow_block_reward(&self, height: u64) -> u64 {
        self.consensus_config.pow.block_reward_at_height(height)
    }

    /// PoW target block time in seconds.
    pub fn pow_target_block_time(&self) -> u64 {
        self.consensus_config.pow.target_block_time_secs
    }

    /// PoS annual staking reward rate.
    pub fn pos_annual_rate(&self) -> f64 {
        self.consensus_config.pos.annual_reward_rate
    }

    /// Initial PoW difficulty in leading-zero bits.
    pub fn initial_difficulty_bits(&self) -> u32 {
        self.consensus_config.pow.initial_difficulty_bits
    }

    /// Coin ticker symbol.
    pub fn ticker(&self) -> &str {
        &self.consensus_config.ticker
    }

    /// Coin name.
    pub fn coin_name(&self) -> &str {
        &self.consensus_config.coin_name
    }

    // -----------------------------------------------------------------
    // Deprecated PoH accessors (kept for any remaining PoH references)
    // -----------------------------------------------------------------

    #[deprecated(note = "PoH removed; use consensus_config.pow instead")]
    pub fn hashes_per_tick(&self) -> Option<u64> {
        None
    }

    #[deprecated(note = "PoH removed; slots are now PoW/PoS block heights")]
    pub fn ticks_per_slot(&self) -> u64 {
        1
    }
}

impl fmt::Display for GenesisConfig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "\
             === Hybrid PoW/PoS Chain Genesis ===\n\
             Coin: {} ({})\n\
             Creation time: {}\n\
             Cluster type: {:?}\n\
             Genesis hash: {}\n\
             Shred version: {}\n\
             --- PoW Parameters ---\n\
             Initial difficulty: {} bits\n\
             PoW block time: {}s\n\
             Initial block reward: {} base units\n\
             Halving interval: {} blocks\n\
             Coinbase maturity: {} blocks\n\
             Difficulty window: {} blocks\n\
             --- PoS Parameters ---\n\
             Min stake: {} base units\n\
             Annual reward rate: {}%\n\
             Min coin age: {}s\n\
             Max coin age: {}s\n\
             --- Accounts ---\n\
             Capitalization: {} coins in {} accounts\n\
             Native instruction processors: {:#?}\n\
             Rewards pool: {:#?}\n\
             ",
            self.consensus_config.coin_name,
            self.consensus_config.ticker,
            Utc.timestamp_opt(self.creation_time, 0)
                .unwrap()
                .to_rfc3339(),
            self.cluster_type,
            self.hash(),
            compute_shred_version(&self.hash(), None),
            self.consensus_config.pow.initial_difficulty_bits,
            self.consensus_config.pow.target_block_time_secs,
            self.consensus_config.pow.initial_block_reward,
            self.consensus_config.pow.halving_interval,
            self.consensus_config.pow.coinbase_maturity,
            self.consensus_config.pow.difficulty_adjustment_window,
            self.consensus_config.pos.min_stake_lamports,
            self.consensus_config.pos.annual_reward_rate * 100.0,
            self.consensus_config.pos.min_coin_age_secs,
            self.consensus_config.pos.max_coin_age_secs,
            lamports_to_sol(
                self.accounts
                    .iter()
                    .map(|(_, account)| account.lamports)
                    .sum::<u64>()
            ),
            self.accounts.len(),
            self.native_instruction_processors,
            self.rewards_pools,
        )
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::signature::{Keypair, Signer},
        std::path::PathBuf,
    };

    fn make_tmp_path(name: &str) -> PathBuf {
        let out_dir = std::env::var("FARF_DIR").unwrap_or_else(|_| "farf".to_string());
        let keypair = Keypair::new();
        let path = [
            out_dir,
            "tmp".to_string(),
            format!("{}-{}", name, keypair.pubkey()),
        ]
        .iter()
        .collect();
        let _ignored = std::fs::remove_dir_all(&path);
        let _ignored = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn test_genesis_config_roundtrip() {
        let faucet_keypair = Keypair::new();
        let mut config = GenesisConfig::default();
        config.add_account(
            faucet_keypair.pubkey(),
            AccountSharedData::new(10_000, 0, &Pubkey::default()),
        );
        config.add_native_instruction_processor("hi".to_string(), solana_sdk::pubkey::new_rand());
        assert_eq!(config.accounts.len(), 1);
        let path = &make_tmp_path("genesis_config");
        config.write(path).expect("write");
        let loaded = GenesisConfig::load(path).expect("load");
        assert_eq!(config.hash(), loaded.hash());
        let _ignored = std::fs::remove_file(path);
    }

    #[test]
    fn test_pow_block_reward_halving() {
        let config = GenesisConfig::default();
        let initial = config.pow_block_reward(0);
        let halved = config.pow_block_reward(210_000);
        assert_eq!(halved, initial / 2);
    }

    #[test]
    fn test_consensus_config_defaults() {
        let config = GenesisConfig::default();
        assert_eq!(config.ticker(), "HYB");
        assert_eq!(config.initial_difficulty_bits(), 20);
        assert!(config.pos_annual_rate() > 0.0);
    }
}
