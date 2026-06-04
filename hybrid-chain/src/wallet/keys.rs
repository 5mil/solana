use sha2::{Sha256, Digest};

/// Derive a simple address from a public key (SHA256 hash truncated to 20 bytes)
/// In production this would use secp256k1 + RIPEMD160 like Bitcoin
pub fn address_from_pubkey(pubkey: &[u8]) -> [u8; 20] {
    let hash = Sha256::digest(pubkey);
    let mut addr = [0u8; 20];
    addr.copy_from_slice(&hash[..20]);
    addr
}

/// Encode address bytes as base58-like hex string (stub — full Base58Check later)
pub fn encode_address(addr: &[u8; 20]) -> String {
    format!("HYB{}", hex::encode(addr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_address_derivation() {
        let pubkey = b"test_public_key_bytes";
        let addr = address_from_pubkey(pubkey);
        assert_eq!(addr.len(), 20);
        let encoded = encode_address(&addr);
        assert!(encoded.starts_with("HYB"));
    }
}
