//! Device-key storage in the OS secure store.
//!
//! The device key encrypts every note at rest. Keeping it in the SQLite DB next
//! to the ciphertext gives no real protection — anyone with the DB file can
//! decrypt. This module moves the key into the platform credential store
//! (Windows Credential Manager / macOS Keychain / Linux secret-service) and
//! scrubs the plaintext DB copy on first run.
//!
//! Android has no `keyring` backend yet, so there the key stays in the
//! app-private DB (see the `cfg(target_os = "android")` variant below). Bridging
//! the Android Keystore is tracked as a follow-up.

#[cfg(not(target_os = "android"))]
use crate::db::queries;
use sqlx::SqlitePool;

const SERVICE: &str = "panote";
const ENTRY: &str = "device_key";

#[cfg(not(target_os = "android"))]
fn to_key(bytes: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    key.copy_from_slice(bytes);
    key
}

/// Store the key in the keychain and, only on success, scrub the plaintext DB
/// copy. If the keychain write fails the key is left in the DB so the app keeps
/// working (degraded security, but no data loss).
#[cfg(not(target_os = "android"))]
async fn store_and_scrub(entry: Option<&keyring::Entry>, pool: &SqlitePool, key: &[u8; 32]) {
    match entry {
        Some(e) => match e.set_secret(key) {
            Ok(()) => {
                let _ = queries::scrub_device_key(pool).await;
            }
            Err(err) => eprintln!("[keystore] keychain write failed, key remains in DB: {err}"),
        },
        None => eprintln!("[keystore] keychain unavailable, key remains in DB"),
    }
}

/// Load the device key, preferring the OS secure store. On first run after this
/// feature ships, migrates the existing DB key into the keychain and blanks the
/// DB copy.
///
/// Critically, once the DB copy has been scrubbed, a subsequent keychain read
/// failure must NOT regenerate a fresh key — that would silently orphan every
/// existing note. We distinguish "first run ever" (no device_config row) from
/// "migrated but keychain now unreadable" (row exists with an empty key) and
/// fail loudly in the latter case.
#[cfg(not(target_os = "android"))]
pub async fn load_or_migrate_device_key(pool: &SqlitePool) -> anyhow::Result<[u8; 32]> {
    let entry = match keyring::Entry::new(SERVICE, ENTRY) {
        Ok(e) => Some(e),
        Err(e) => {
            eprintln!("[keystore] entry init failed: {e}");
            None
        }
    };

    // 1. Try the keychain.
    if let Some(e) = &entry {
        match e.get_secret() {
            Ok(bytes) if bytes.len() == 32 => return Ok(to_key(&bytes)),
            Ok(_) => eprintln!("[keystore] ignoring malformed keychain secret"),
            Err(keyring::Error::NoEntry) => {}
            Err(err) => {
                // Transient read failure. Fall back to the DB key if it's still
                // there; never regenerate.
                if let Some(bytes) = queries::device_key_raw(pool).await? {
                    if bytes.len() == 32 {
                        eprintln!("[keystore] keychain read failed, using DB key: {err}");
                        return Ok(to_key(&bytes));
                    }
                }
                anyhow::bail!("could not read device key from OS keychain: {err}");
            }
        }
    }

    // 2. Keychain has no usable key — consult the DB.
    match queries::device_key_raw(pool).await? {
        // DB still holds the key: migrate it into the keychain.
        Some(bytes) if bytes.len() == 32 => {
            let key = to_key(&bytes);
            store_and_scrub(entry.as_ref(), pool, &key).await;
            Ok(key)
        }
        // Row exists but the key was scrubbed and the keychain lost it — the key
        // is gone. Refuse rather than mint a new one that can't decrypt anything.
        Some(_) => anyhow::bail!(
            "device key was moved to the OS keychain but is no longer present there; \
             notes cannot be decrypted"
        ),
        // No row at all: genuine first run. Generate, persist to DB, migrate.
        None => {
            let key = queries::get_or_create_device_key(pool).await?;
            store_and_scrub(entry.as_ref(), pool, &key).await;
            Ok(key)
        }
    }
}

/// Android fallback: no OS keyring backend available. The key stays in the
/// app-private SQLite DB, protected by Android's per-app storage sandbox and
/// file-based encryption. Bridging the Android Keystore is a follow-up.
#[cfg(target_os = "android")]
pub async fn load_or_migrate_device_key(pool: &SqlitePool) -> anyhow::Result<[u8; 32]> {
    crate::db::queries::get_or_create_device_key(pool).await
}
