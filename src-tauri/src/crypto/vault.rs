use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    ChaCha20Poly1305, Nonce,
};
use rand::{rngs::OsRng, RngCore};

/// Argon2id parallelism. Current derivations use p=4 (K2); notes protected
/// before the bump were derived at p=1 and are read via `derive_key_legacy`,
/// then re-wrapped at the current params on next unlock.
const KDF_PARALLELISM: u32 = 4;
const KDF_PARALLELISM_LEGACY: u32 = 1;

/// Argon2 is deliberately slow, and every password path calls it from an async
/// command. On the app's multi-thread runtime, tell tokio this worker is busy so
/// other commands keep moving; off a runtime, or on a test's single-thread one,
/// there is nowhere else to go, so it just runs.
fn cpu_bound<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current().map(|h| h.runtime_flavor()) {
        Ok(tokio::runtime::RuntimeFlavor::MultiThread) => tokio::task::block_in_place(f),
        _ => f(),
    }
}

fn derive_key_with(passphrase: &str, salt: &[u8], parallelism: u32) -> anyhow::Result<[u8; 32]> {
    let params = Params::new(65536, 3, parallelism, Some(32))
        .map_err(|e| anyhow::anyhow!("argon2 params: {e}"))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    cpu_bound(|| argon2.hash_password_into(passphrase.as_bytes(), salt, &mut key))
        .map_err(|e| anyhow::anyhow!("argon2 hash: {e}"))?;
    Ok(key)
}

/// Derive a 32-byte key from a passphrase + salt using Argon2id (current params).
pub fn derive_key(passphrase: &str, salt: &[u8]) -> anyhow::Result<[u8; 32]> {
    derive_key_with(passphrase, salt, KDF_PARALLELISM)
}

/// Derive using the pre-K2 params (p=1). Only for reading notes protected before
/// the parallelism bump — never for new writes.
pub fn derive_key_legacy(passphrase: &str, salt: &[u8]) -> anyhow::Result<[u8; 32]> {
    derive_key_with(passphrase, salt, KDF_PARALLELISM_LEGACY)
}

/// Generate a fresh 32-byte random salt.
pub fn random_salt() -> [u8; 32] {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// Generate a fresh 12-byte random nonce.
pub fn random_nonce() -> [u8; 12] {
    let mut buf = [0u8; 12];
    OsRng.fill_bytes(&mut buf);
    buf
}

/// Encrypt plaintext with a 32-byte key, binding `aad` (additional authenticated
/// data — e.g. a note id) so ciphertext can't be swapped between rows/contexts.
/// Returns (nonce, ciphertext).
pub fn encrypt(
    key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],
) -> anyhow::Result<([u8; 12], Vec<u8>)> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce_bytes = random_nonce();
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(
            nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|e| anyhow::anyhow!("encrypt: {e}"))?;
    Ok((nonce_bytes, ct))
}

/// Decrypt ciphertext with a 32-byte key + nonce bytes, verifying `aad`.
pub fn decrypt(
    key: &[u8; 32],
    nonce_bytes: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new(key.into());
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
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
        let (nonce, ct) = encrypt(&key, plaintext, b"aad").unwrap();
        let recovered = decrypt(&key, &nonce, &ct, b"aad").unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn decrypt_wrong_key_fails() {
        let key = derive_key("correct", &[0u8; 16]).unwrap();
        let wrong_key = derive_key("wrong", &[0u8; 16]).unwrap();
        let (nonce, ct) = encrypt(&key, b"secret", b"aad").unwrap();
        assert!(decrypt(&wrong_key, &nonce, &ct, b"aad").is_err());
    }

    #[test]
    fn decrypt_tampered_ciphertext_fails() {
        let key = derive_key("test", &[0u8; 16]).unwrap();
        let (nonce, mut ct) = encrypt(&key, b"secret", b"aad").unwrap();
        ct[0] ^= 0xff;
        assert!(decrypt(&key, &nonce, &ct, b"aad").is_err());
    }

    #[test]
    fn decrypt_wrong_aad_fails() {
        // N3: ciphertext produced for one context (e.g. note id) must not
        // decrypt under a different context — prevents ciphertext swapping.
        let key = derive_key("test", &[0u8; 16]).unwrap();
        let (nonce, ct) = encrypt(&key, b"secret", b"note-a").unwrap();
        assert!(decrypt(&key, &nonce, &ct, b"note-b").is_err());
    }

    #[test]
    fn random_salt_is_32_bytes() {
        assert_eq!(random_salt().len(), 32);
    }

    #[test]
    fn legacy_params_derive_a_different_key() {
        // K2: p=1 (legacy) and p=4 (current) must produce different keys for the
        // same password+salt, so the fallback path is actually exercised.
        let salt = [7u8; 16];
        assert_ne!(
            derive_key("pw", &salt).unwrap(),
            derive_key_legacy("pw", &salt).unwrap()
        );
    }
}
