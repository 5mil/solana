//! Solana SDK — modified for SHA256d PoW/PoS hybrid consensus.
//! Proof of History (PoH) has been removed from the consensus path.
//! See `consensus.rs` for the new hybrid consensus types.

#![cfg_attr(RUSTC_WITH_SPECIALIZATION, feature(min_specialization))]

// Macros first
#[macro_use]
pub mod log;

extern crate self as solana_sdk;

pub use solana_program::{
    account_info,
    big_mod_exp,
    blake3,
    borsh,
    borsh0_10,
    bpf_loader,
    bpf_loader_deprecated,
    bpf_loader_upgradeable,
    clock,
    compute_budget,
    config,
    custom_heap_default,
    custom_panic_default,
    debug_account_data,
    decode_error,
    ed25519_program,
    entrypoint,
    entrypoint_deprecated,
    epoch_schedule,
    fee_calculator,
    impl_deserialize_as_private_key_of_keypair,
    incinerator,
    instruction,
    keccak,
    lamports,
    loader_instruction,
    loader_upgradeable_instruction,
    loader_v4,
    loader_v4_instruction,
    message,
    msg,
    native_token,
    nonce,
    poseidon,
    program,
    program_error,
    program_memory,
    program_option,
    program_pack,
    pubkey,
    rent,
    sanitize,
    secp256k1_program,
    secp256k1_recover,
    secp_utils,
    serialize_utils,
    short_vec,
    slot_hashes,
    slot_history,
    stable_layout,
    stake,
    stake_history,
    syscalls,
    system_instruction,
    system_program,
    sysvar,
    vote,
    wasm_bindgen,
};

pub mod account;
pub mod account_utils;
pub mod client;
pub mod commitment_config;
pub mod compute_budget;
/// Hybrid SHA256d PoW/PoS consensus types (replaces PoH in consensus path)
pub mod consensus;
pub mod derivation_path;
pub mod deserialize_utils;
pub mod ed25519_instruction;
pub mod epoch_info;
pub mod epoch_rewards_hasher;
pub mod example_mocks;
pub mod exit;
pub mod feature;
pub mod feature_set;
pub mod fee;
pub mod genesis_config;
pub mod hard_forks;
pub mod hash;
pub mod inflation;
pub mod inner_instruction;
pub mod native_loader;
pub mod net;
pub mod nonce_account;
pub mod nonce_info;
pub mod offchain_message;
pub mod packet;
/// Retained as deprecated stub for ABI compatibility — use `consensus` instead
pub mod poh_config;
pub mod precompiles;
pub mod program_utils;
pub mod quic;
pub mod recent_blockhashes_account;
pub mod rent_collector;
pub mod rent_debits;
pub mod reserved_account_keys;
pub mod reward_info;
pub mod reward_type;
pub mod rpc_port;
pub mod secp256k1_instruction;
pub mod shred_version;
pub mod signature;
pub mod signer;
pub mod simple_vote_transaction_checker;
pub mod system_transaction;
pub mod timing;
pub mod transaction;
pub mod transaction_context;
pub mod transport;

/// Re-export the hybrid consensus config as the canonical chain config type.
pub use consensus::{
    BlockConsensusData,
    ConsensusType,
    HybridConsensusConfig,
    PosConfig,
    PowConfig,
    hash_meets_difficulty,
    retarget_difficulty,
};

/// Current SDK version
#[macro_export]
macro_rules! sdk_version {
    () => {
        env!("CARGO_PKG_VERSION")
    };
}
