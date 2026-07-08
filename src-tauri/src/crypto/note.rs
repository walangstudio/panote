use super::vault::{decrypt, derive_key, derive_key_legacy, encrypt, random_salt};

/// Encrypt note content with the vault key, binding `aad` (e.g. the note id)
/// so ciphertext can't be swapped between rows/fields (N3).
/// Returns (nonce, ciphertext).
pub fn encrypt_with_vault(
    vault_key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],
) -> anyhow::Result<([u8; 12], Vec<u8>)> {
    encrypt(vault_key, plaintext, aad)
}

/// Decrypt note content with the vault key, verifying `aad`.
// ponytail: rows written before AAD binding was added have no AAD baked in.
// Retry with an empty AAD so those legacy rows still read; a future migration
// should re-encrypt every row with its real AAD and drop this fallback.
pub fn decrypt_with_vault(
    vault_key: &[u8; 32],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> anyhow::Result<Vec<u8>> {
    decrypt(vault_key, nonce, ciphertext, aad).or_else(|_| decrypt(vault_key, nonce, ciphertext, b""))
}

/// Apply an additional per-note encryption layer on top of already-vault-encrypted bytes.
/// Returns (note_salt, note_nonce, double-encrypted ciphertext).
pub fn apply_note_password(
    password: &str,
    vault_ct: &[u8],
) -> anyhow::Result<([u8; 32], [u8; 12], Vec<u8>)> {
    let salt = random_salt();
    let key = derive_key(password, &salt)?;
    let (nonce, ct) = encrypt(&key, vault_ct, b"")?;
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
    Ok(remove_note_password_detect(password, note_salt, note_nonce, double_ct)?.0)
}

/// Like `remove_note_password`, but also reports whether the note was derived at
/// the legacy Argon2 params (p=1). Callers use the flag to re-wrap the note at
/// the current params (K2 upgrade-on-unlock).
pub fn remove_note_password_detect(
    password: &str,
    note_salt: &[u8],
    note_nonce: &[u8],
    double_ct: &[u8],
) -> anyhow::Result<(Vec<u8>, bool)> {
    let key = derive_key(password, note_salt)?;
    if let Ok(pt) = decrypt(&key, note_nonce, double_ct, b"") {
        return Ok((pt, false));
    }
    // ponytail: notes protected before the K2 p=1→p=4 bump were derived at p=1.
    // Retry so they still unlock; the caller re-wraps them at p=4 on next unlock.
    let legacy = derive_key_legacy(password, note_salt)?;
    Ok((decrypt(&legacy, note_nonce, double_ct, b"")?, true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::vault::{derive_key, derive_key_legacy, encrypt};

    fn make_key(pw: &str) -> [u8; 32] {
        derive_key(pw, &[0u8; 16]).unwrap()
    }

    #[test]
    fn legacy_p1_note_unlocks_and_is_flagged() {
        // A note protected before the K2 bump: password layer derived at p=1.
        let vault_key = make_key("vault-pass");
        let (vnonce, vault_ct) = encrypt_with_vault(&vault_key, b"legacy secret", b"note-1").unwrap();
        let salt = [3u8; 16];
        let legacy_key = derive_key_legacy("pw", &salt).unwrap();
        let (nnonce, double_ct) = encrypt(&legacy_key, &vault_ct, b"").unwrap();

        let (recovered_vc, was_legacy) =
            remove_note_password_detect("pw", &salt, &nnonce, &double_ct).unwrap();
        assert!(was_legacy, "p=1 note should be flagged for upgrade");
        let recovered = decrypt_with_vault(&vault_key, &vnonce, &recovered_vc, b"note-1").unwrap();
        assert_eq!(recovered, b"legacy secret");
    }

    #[test]
    fn current_p4_note_is_not_flagged_legacy() {
        let (salt, nonce, ct) = apply_note_password("pw", b"vault-ct-bytes").unwrap();
        let (vc, was_legacy) = remove_note_password_detect("pw", &salt, &nonce, &ct).unwrap();
        assert!(!was_legacy);
        assert_eq!(vc, b"vault-ct-bytes");
    }

    #[test]
    fn note_password_roundtrip() {
        let vault_key = make_key("vault-pass");
        let plaintext = b"{\"body\":\"secret note content\"}";

        let (nonce, vault_ct) = encrypt_with_vault(&vault_key, plaintext, b"note-1").unwrap();
        let (salt, note_nonce, double_ct) = apply_note_password("note-pass", &vault_ct).unwrap();

        // Unwrap per-note layer
        let recovered_vault_ct =
            remove_note_password("note-pass", &salt, &note_nonce, &double_ct).unwrap();
        // Unwrap vault layer
        let recovered =
            decrypt_with_vault(&vault_key, &nonce, &recovered_vault_ct, b"note-1").unwrap();

        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn wrong_note_password_fails() {
        let vault_key = make_key("vault-pass");
        let (_, vault_ct) = encrypt_with_vault(&vault_key, b"data", b"note-1").unwrap();
        let (salt, note_nonce, double_ct) = apply_note_password("correct", &vault_ct).unwrap();
        assert!(remove_note_password("wrong", &salt, &note_nonce, &double_ct).is_err());
    }

    #[test]
    fn notes_without_password_roundtrip() {
        let vault_key = make_key("vault-pass");
        let plaintext = b"plain note, no per-note password";
        let (nonce, ct) = encrypt_with_vault(&vault_key, plaintext, b"note-2").unwrap();
        let recovered = decrypt_with_vault(&vault_key, &nonce, &ct, b"note-2").unwrap();
        assert_eq!(recovered, plaintext);
    }

    #[test]
    fn decrypt_with_vault_wrong_aad_fails() {
        let vault_key = make_key("vault-pass");
        let (nonce, ct) = encrypt_with_vault(&vault_key, b"secret", b"note-a").unwrap();
        assert!(decrypt_with_vault(&vault_key, &nonce, &ct, b"note-b").is_err());
    }

    #[test]
    fn decrypt_with_vault_legacy_no_aad_self_heals() {
        // Rows written before AAD binding used empty AAD implicitly.
        let vault_key = make_key("vault-pass");
        let (nonce, ct) = encrypt_with_vault(&vault_key, b"legacy row", b"").unwrap();
        let recovered = decrypt_with_vault(&vault_key, &nonce, &ct, b"note-1").unwrap();
        assert_eq!(recovered, b"legacy row");
    }
}
