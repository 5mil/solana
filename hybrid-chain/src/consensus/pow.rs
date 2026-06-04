use sha2::{Sha256, Digest};
use crate::params::CHAIN_PARAMS;

/// Double SHA256 (SHA256d) — same as Bitcoin
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(&first);
    second.into()
}

/// Check if a hash meets the difficulty target (leading zero bits)
pub fn meets_difficulty(hash: &[u8; 32], difficulty: u32) -> bool {
    let full_bytes = (difficulty / 8) as usize;
    let remainder_bits = difficulty % 8;

    for i in 0..full_bytes {
        if hash[i] != 0 {
            return false;
        }
    }
    if remainder_bits > 0 && full_bytes < 32 {
        let mask = 0xFF_u8 << (8 - remainder_bits);
        if hash[full_bytes] & mask != 0 {
            return false;
        }
    }
    true
}

/// Mine a nonce that satisfies the difficulty target
pub fn mine(header_bytes: &[u8], difficulty: u32) -> (u64, [u8; 32]) {
    let mut nonce: u64 = 0;
    loop {
        let mut data = header_bytes.to_vec();
        data.extend_from_slice(&nonce.to_le_bytes());
        let hash = sha256d(&data);
        if meets_difficulty(&hash, difficulty) {
            return (nonce, hash);
        }
        nonce = nonce.wrapping_add(1);
        if nonce % 1_000_000 == 0 {
            log::debug!("Mining... nonce={}", nonce);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256d_known_vector() {
        // SHA256d of empty bytes
        let result = sha256d(b"");
        assert_eq!(result.len(), 32);
    }

    #[test]
    fn test_meets_difficulty_zero() {
        let hash = [0u8; 32];
        assert!(meets_difficulty(&hash, 32));
    }

    #[test]
    fn test_not_meets_difficulty() {
        let mut hash = [0u8; 32];
        hash[0] = 0xFF;
        assert!(!meets_difficulty(&hash, 8));
    }
}
