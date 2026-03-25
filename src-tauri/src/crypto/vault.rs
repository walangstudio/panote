use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::{rngs::OsRng, RngCore};

/// Derive a 32-byte key from a passphrase + salt using Argon2id.
pub fn derive_key(passphrase: &str, salt: &[u8]) -> anyhow::Result<[u8; 32]> {
    let params = Params::new(65536, 3, 1, Some(32))
        .map_err(|e| anyhow::anyhow!("argon2 params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow::anyhow!("argon2 hash: {e}"))?;
    Ok(key)
}

/// Generate a fresh 16-byte random salt.
pub fn random_salt() -> [u8; 16] {
    let mut buf = [0u8; 16];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// Generate a fresh 12-byte random nonce.
pub fn random_nonce() -> [u8; 12] {
    let mut buf = [0u8; 12];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// Encrypt plaintext with a 32-byte key. Returns (nonce, ciphertext).
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> anyhow::Result<([u8; 12], Vec<u8>)> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce_bytes = random_nonce();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;
    Ok((nonce_bytes, ct))
}

/// Decrypt ciphertext with a 32-byte key + nonce bytes.
pub fn decrypt(key: &[u8; 32], nonce_bytes: &[u8], ciphertext: &[u8]) -> anyhow::Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| anyhow::anyhow!("decryption failed — wrong key or corrupted data"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_key_is_deterministic() {
        let salt = [0u8; 16];
        let k1 = derive_key("hunter2", &salt).unwrap();
        let k2 = derive_key("hunter2", &salt).unwrap();
        assert_eq!(k1, k2);
    }

    #[test]
    fn derive_key_differs_by_passphrase() {
        let salt = [0u8; 16];
        let k1 = derive_key("password1", &salt).unwrap();
        let k2 = derive_key("password2", &salt).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn derive_key_differs_by_salt() {
        let k1 = derive_key("password", &[0u8; 16]).unwrap();
        let k2 = derive_key("password", &[1u8; 16]).unwrap();
        assert_ne!(k1, k2);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = derive_key("test", &[42u8; 16]).unwrap();
        let plaintext = b"hello panote";
        let (nonce, ct) = encrypt(&key, plaintext).unwrap();
        let recovered = decrypt(&key, &nonce, &ct).unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn decrypt_wrong_key_fails() {
        let key = derive_key("correct", &[0u8; 16]).unwrap();
        let wrong_key = derive_key("wrong", &[0u8; 16]).unwrap();
        let (nonce, ct) = encrypt(&key, b"secret").unwrap();
        assert!(decrypt(&wrong_key, &nonce, &ct).is_err());
    }

    #[test]
    fn decrypt_tampered_ciphertext_fails() {
        let key = derive_key("test", &[0u8; 16]).unwrap();
        let (nonce, mut ct) = encrypt(&key, b"secret").unwrap();
        ct[0] ^= 0xff;
        assert!(decrypt(&key, &nonce, &ct).is_err());
    }
}
