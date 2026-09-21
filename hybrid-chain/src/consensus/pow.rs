use sha2::{Digest, Sha256};

/// Double SHA256 (SHA256d) — same as Bitcoin
pub fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    second.into()
}

/// Check if a hash meets the difficulty target (leading zero bits)
pub fn meets_difficulty(hash: &[u8; 32], difficulty: u32) -> bool {
    let difficulty = difficulty.min(256);
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

/// Mine a nonce such that SHA256d(prefix || nonce_le) meets difficulty.
pub fn mine(header_prefix: &[u8], difficulty: u32) -> (u64, [u8; 32]) {
    let mut nonce: u64 = 0;
    loop {
        let mut data = header_prefix.to_vec();
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
        let result = sha256d(b"");
        let expected =
            hex::decode("5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456")
                .unwrap();
        assert_eq!(result.to_vec(), expected);
    }

    #[test]
    fn test_sha256d_deterministic() {
        assert_eq!(sha256d(b"hybrid"), sha256d(b"hybrid"));
    }

    #[test]
    fn accepts_hash_equal_or_below_target() {
        let hash = [0u8; 32];
        assert!(meets_difficulty(&hash, 32));
        assert!(meets_difficulty(&hash, 1));
    }

    #[test]
    fn rejects_hash_above_target() {
        let mut hash = [0u8; 32];
        hash[0] = 0xFF;
        assert!(!meets_difficulty(&hash, 8));
    }

    #[test]
    fn mine_finds_valid_nonce() {
        let (nonce, hash) = mine(b"test-header", 12);
        let mut data = b"test-header".to_vec();
        data.extend_from_slice(&nonce.to_le_bytes());
        assert_eq!(sha256d(&data), hash);
        assert!(meets_difficulty(&hash, 12));
    }

    #[test]
    fn different_prefix_different_hash() {
        let h1 = sha256d(b"parent-a");
        let h2 = sha256d(b"parent-b");
        assert_ne!(h1, h2);
    }
}
