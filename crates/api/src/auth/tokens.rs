//! Opaque refresh-token generation + hashing. We persist only the SHA-256 hash.

use rand::RngCore;
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

/// Generate a cryptographically-random opaque refresh token (256 bits, hex-encoded).
pub fn generate_refresh_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// SHA-256 hash of a refresh token, as stored in the database.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_unique_and_hex() {
        let a = generate_refresh_token();
        let b = generate_refresh_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64); // 32 bytes hex
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hashing_is_deterministic_and_hides_the_token() {
        let token = generate_refresh_token();
        assert_eq!(hash_token(&token), hash_token(&token));
        assert_ne!(hash_token(&token), token);
    }
}
