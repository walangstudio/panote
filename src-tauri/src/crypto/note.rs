use super::vault::{decrypt, derive_key, encrypt, random_salt};

/// Encrypt note content with the vault key.
/// Returns (nonce, ciphertext).
pub fn encrypt_with_vault(
    vault_key: &[u8; 32],
    plaintext: &[u8],
) -> anyhow::Result<([u8; 12], Vec<u8>)> {
    encrypt(vault_key, plaintext)
}

/// Decrypt note content with the vault key.
pub fn decrypt_with_vault(
    vault_key: &[u8; 32],
    nonce: &[u8],
    ciphertext: &[u8],
) -> anyhow::Result<Vec<u8>> {
    decrypt(vault_key, nonce, ciphertext)
}

/// Apply an additional per-note encryption layer on top of already-vault-encrypted bytes.
/// Returns (note_salt, note_nonce, double-encrypted ciphertext).
pub fn apply_note_password(
    password: &str,
    vault_ct: &[u8],
) -> anyhow::Result<([u8; 16], [u8; 12], Vec<u8>)> {
    let salt = random_salt();
    let key = derive_key(password, &salt)?;
    let (nonce, ct) = encrypt(&key, vault_ct)?;
    Ok((salt, nonce, ct))
}

/// Recover the vault ciphertext from a (possibly) protected note row.
/// If the note has no password layer, returns the content as-is. If it does,
/// `password` must be supplied (`None` → `Err`), and a wrong password also errors.
/// Shared by note_get, transfer send, and export so the peel logic lives once.
pub fn peel_vault_ct(
    note_salt: Option<&[u8]>,
    note_nonce: Option<&[u8]>,
    content_ct: &[u8],
    password: Option<&str>,
) -> anyhow::Result<Vec<u8>> {
    match (note_salt, note_nonce) {
        (Some(salt), Some(nonce)) => {
            let pw = password.ok_or_else(|| anyhow::anyhow!("locked"))?;
            remove_note_password(pw, salt, nonce, content_ct)
        }
        _ => Ok(content_ct.to_vec()),
    }
}

/// Strip the per-note encryption layer. Returns the vault-encrypted bytes.
pub fn remove_note_password(
    password: &str,
    note_salt: &[u8],
    note_nonce: &[u8],
    double_ct: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let key = derive_key(password, note_salt)?;
    decrypt(&key, note_nonce, double_ct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::vault::derive_key;

    fn make_key(pw: &str) -> [u8; 32] {
        derive_key(pw, &[0u8; 16]).unwrap()
    }

    #[test]
    fn note_password_roundtrip() {
        let vault_key = make_key("vault-pass");
        let plaintext = b"{\"body\":\"secret note content\"}";

        let (nonce, vault_ct) = encrypt_with_vault(&vault_key, plaintext).unwrap();
        let (salt, note_nonce, double_ct) = apply_note_password("note-pass", &vault_ct).unwrap();

        // Unwrap per-note layer
        let recovered_vault_ct =
            remove_note_password("note-pass", &salt, &note_nonce, &double_ct).unwrap();
        // Unwrap vault layer
        let recovered = decrypt_with_vault(&vault_key, &nonce, &recovered_vault_ct).unwrap();

        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn wrong_note_password_fails() {
        let vault_key = make_key("vault-pass");
        let (_, vault_ct) = encrypt_with_vault(&vault_key, b"data").unwrap();
        let (salt, note_nonce, double_ct) = apply_note_password("correct", &vault_ct).unwrap();
        assert!(remove_note_password("wrong", &salt, &note_nonce, &double_ct).is_err());
    }

    #[test]
    fn notes_without_password_roundtrip() {
        let vault_key = make_key("vault-pass");
        let plaintext = b"plain note, no per-note password";
        let (nonce, ct) = encrypt_with_vault(&vault_key, plaintext).unwrap();
        let recovered = decrypt_with_vault(&vault_key, &nonce, &ct).unwrap();
        assert_eq!(recovered, plaintext);
    }
}
