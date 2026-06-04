//! `solana-runtime` — modified for hybrid PoW/PoS consensus.
//! `block_validator` added as the consensus proof validation hook.

#[macro_use]
extern crate log;

#[macro_use]
extern crate solana_metrics;

pub mod accounts;
pub mod accounts_background_service;
pub mod accounts_cache;
pub mod accounts_db;
pub mod accounts_hash;
pub mod accounts_index;
pub mod accounts_index_storage;
pub mod accounts_partition;
pub mod accounts_update_notifier_interface;
pub mod ancient_append_vecs;
pub mod append_vec;
pub mod bank;
pub mod bank_client;
pub mod bank_forks;
/// Hybrid PoW/PoS block consensus proof validator.
pub mod block_validator;
pub mod builtins;
pub mod commitment;
pub mod compute_budget;
pub mod cost_model;
pub mod cost_tracker;
pub mod epoch_accounts_hash;
pub mod epoch_rewards_hasher;
pub mod epoch_stakes;
pub mod expected_rent_collection;
pub mod fee_components;
pub mod genesis_utils;
pub mod hardened_unpack;
pub mod installed_scheduler_pool;
pub mod loader_utils;
pub mod message_processor;
pub mod non_circulating_supply;
pub mod prioritization_fee;
pub mod prioritization_fee_cache;
pub mod read_only_accounts_cache;
pub mod rent_collector;
pub mod rent_paying_accounts_by_partition;
pub mod runtime_config;
pub mod serde_snapshot;
pub mod snapshot_archive_info;
pub mod snapshot_bank_utils;
pub mod snapshot_config;
pub mod snapshot_hash;
pub mod snapshot_minimizer;
pub mod snapshot_package;
pub mod snapshot_utils;
pub mod stake_account;
pub mod stake_history;
pub mod stake_weighted_timestamp;
pub mod stakes;
pub mod status_cache;
pub mod system_instruction_processor;
pub mod timings;
pub mod transaction_batch;
pub mod transaction_error_metrics;
pub mod transaction_priority_details;
pub mod verify_precompiles;

pub use block_validator::{validate_block_consensus, BlockValidationError, max_coinbase_reward};
