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

/// AAD for the title's password layer. The body's layer binds none, so this
/// keeps a note's two sealed blobs from being swapped for each other.
const TITLE_AAD: &[u8] = b"panote-note-title";

/// A protected note's body and title, sealed under one key (one salt) so a
/// single Argon2 derivation opens both.
pub struct SealedNote {
    pub salt: [u8; 32],
    pub nonce: [u8; 12],
    pub content_ct: Vec<u8>,
    pub title_nonce: [u8; 12],
    pub title_ct: Vec<u8>,
}

/// Seal already-vault-encrypted body and title under a fresh note-password key.
pub fn seal_note(password: &str, vault_ct: &[u8], vault_title_ct: &[u8]) -> anyhow::Result<SealedNote> {
    let salt = random_salt();
    let key = derive_key(password, &salt)?;
    let (nonce, content_ct) = encrypt(&key, vault_ct, b"")?;
    let (title_nonce, title_ct) = encrypt(&key, vault_title_ct, TITLE_AAD)?;
    Ok(SealedNote { salt, nonce, content_ct, title_nonce, title_ct })
}

/// Reverse of [`seal_note`]: returns (vault_ct, vault title ct, was_legacy).
/// `sealed_title` is (title nonce, title ct), or `None` for a note protected
/// before titles were sealed, whose title is still under the device key only.
pub fn open_note(
    password: &str,
    note_salt: &[u8],
    note_nonce: &[u8],
    double_ct: &[u8],
    sealed_title: Option<(&[u8], &[u8])>,
) -> anyhow::Result<(Vec<u8>, Option<Vec<u8>>, bool)> {
    let (key, vault_ct, was_legacy) = open_body(password, note_salt, note_nonce, double_ct)?;
    let title = sealed_title
        .map(|(nonce, ct)| decrypt(&key, nonce, ct, TITLE_AAD))
        .transpose()?;
    Ok((vault_ct, title, was_legacy))
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
    let (_, pt, was_legacy) = open_body(password, note_salt, note_nonce, double_ct)?;
    Ok((pt, was_legacy))
}

/// Peel the body's password layer, returning the key that opened it too.
fn open_body(
    password: &str,
    note_salt: &[u8],
    note_nonce: &[u8],
    double_ct: &[u8],
) -> anyhow::Result<([u8; 32], Vec<u8>, bool)> {
    let key = derive_key(password, note_salt)?;
    if let Ok(pt) = decrypt(&key, note_nonce, double_ct, b"") {
        return Ok((key, pt, false));
    }
    // ponytail: notes protected before the K2 p=1→p=4 bump were derived at p=1.
    // Retry so they still unlock; the caller re-wraps them at p=4 on next unlock.
    let legacy = derive_key_legacy(password, note_salt)?;
    let pt = decrypt(&legacy, note_nonce, double_ct, b"")?;
    Ok((legacy, pt, true))
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
    fn sealed_note_opens_body_and_title_under_one_password() {
        let s = seal_note("pw", b"body-ct", b"title-ct").unwrap();
        let (body, title, legacy) =
            open_note("pw", &s.salt, &s.nonce, &s.content_ct, Some((&s.title_nonce, &s.title_ct)))
                .unwrap();
        assert_eq!((body.as_slice(), title.as_deref(), legacy), (&b"body-ct"[..], Some(&b"title-ct"[..]), false));
        assert!(open_note("wrong", &s.salt, &s.nonce, &s.content_ct, None).is_err());
    }

    /// Distinct AAD: the sealed title can't be passed off as the body or back.
    #[test]
    fn sealed_title_and_body_cannot_be_swapped() {
        let s = seal_note("pw", b"body-ct", b"title-ct").unwrap();
        assert!(open_note("pw", &s.salt, &s.title_nonce, &s.title_ct, None).is_err());
        assert!(open_note("pw", &s.salt, &s.nonce, &s.content_ct, Some((&s.nonce, &s.content_ct))).is_err());
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
