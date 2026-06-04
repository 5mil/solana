use serde::{Deserialize, Serialize};
use crate::consensus::pow::sha256d;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub prev_txid: [u8; 32],
    pub prev_vout: u32,
    pub script_sig: Vec<u8>, // signature data
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub value: u64,          // amount in base units
    pub script_pubkey: Vec<u8>, // locking script / address hash
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u32,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub locktime: u32,
    /// PoS coinstake flag: true if this is a staking transaction
    pub is_coinstake: bool,
}

impl Transaction {
    /// Compute the transaction ID (SHA256d of serialized tx)
    pub fn txid(&self) -> [u8; 32] {
        let bytes = bincode::serialize(self).unwrap_or_default();
        sha256d(&bytes)
    }

    /// Create a coinbase transaction (PoW block reward)
    pub fn coinbase(block_height: u64, reward: u64, miner_address: &str) -> Self {
        Transaction {
            version: 1,
            inputs: vec![TxInput {
                prev_txid: [0u8; 32],
                prev_vout: 0xFFFF_FFFF,
                script_sig: block_height.to_le_bytes().to_vec(),
            }],
            outputs: vec![TxOutput {
                value: reward,
                script_pubkey: miner_address.as_bytes().to_vec(),
            }],
            locktime: 0,
            is_coinstake: false,
        }
    }

    /// Create a coinstake transaction (PoS block reward)
    pub fn coinstake(stake_coins: u64, reward: u64, staker_address: &str) -> Self {
        Transaction {
            version: 1,
            inputs: vec![TxInput {
                prev_txid: [0u8; 32],
                prev_vout: 0,
                script_sig: vec![],
            }],
            outputs: vec![
                TxOutput { value: 0, script_pubkey: vec![] }, // first output empty (coinstake marker)
                TxOutput {
                    value: stake_coins + reward,
                    script_pubkey: staker_address.as_bytes().to_vec(),
                },
            ],
            locktime: 0,
            is_coinstake: true,
        }
    }
}
