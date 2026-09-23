use serde::{Deserialize, Serialize};
use crate::consensus::pow::sha256d;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxInput {
    pub prev_txid: [u8; 32],
    pub prev_vout: u32,
    pub script_sig: Vec<u8>,
}

/// Classic output. `value` is always zero — amounts live in compact commitments.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxOutput {
    pub value: u64,
    pub script_pubkey: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub version: u32,
    pub inputs: Vec<TxInput>,
    pub outputs: Vec<TxOutput>,
    pub locktime: u32,
    pub is_coinstake: bool,
}

impl Transaction {
    pub fn txid(&self) -> [u8; 32] {
        let bytes = bincode::serialize(self).unwrap_or_default();
        sha256d(&bytes)
    }

    pub fn coinbase(block_height: u64, _reward: u64, dest: &[u8]) -> Self {
        Transaction {
            version: 1,
            inputs: vec![TxInput {
                prev_txid: [0u8; 32],
                prev_vout: 0xFFFF_FFFF,
                script_sig: block_height.to_le_bytes().to_vec(),
            }],
            outputs: vec![TxOutput {
                value: 0,
                script_pubkey: dest.to_vec(),
            }],
            locktime: 0,
            is_coinstake: false,
        }
    }

    pub fn coinstake(_stake_coins: u64, _reward: u64, dest: &[u8]) -> Self {
        Transaction {
            version: 1,
            inputs: vec![TxInput {
                prev_txid: [0u8; 32],
                prev_vout: 0,
                script_sig: vec![],
            }],
            outputs: vec![
                TxOutput { value: 0, script_pubkey: vec![] },
                TxOutput {
                    value: 0,
                    script_pubkey: dest.to_vec(),
                },
            ],
            locktime: 0,
            is_coinstake: true,
        }
    }
}
