use rand::{rngs::OsRng, RngCore};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

// ----- Device config -----

/// Load the device key, generating and persisting a new random one on first launch.
pub async fn get_or_create_device_key(pool: &SqlitePool) -> anyhow::Result<[u8; 32]> {
    if let Some(bytes) = device_key_raw(pool).await? {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    set_device_key(pool, &key).await?;
    Ok(key)
}

/// Raw device_key blob from the device_config row. `None` if the row doesn't
/// exist (genuine first run); `Some(empty)` after the key was scrubbed into the
/// OS keychain. Callers must distinguish these to avoid minting a fresh key
/// (which would orphan every existing note).
pub async fn device_key_raw(pool: &SqlitePool) -> anyhow::Result<Option<Vec<u8>>> {
    let row = sqlx::query("SELECT device_key FROM device_config WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get::<Vec<u8>, _>("device_key")))
}

/// Insert or replace the device key. Upsert so it never panics on the
/// single-row PRIMARY KEY when the row already exists.
pub async fn set_device_key(pool: &SqlitePool, key: &[u8]) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO device_config (id, device_key) VALUES (1, ?)
         ON CONFLICT(id) DO UPDATE SET device_key = excluded.device_key",
    )
    .bind(key)
    .execute(pool)
    .await?;
    Ok(())
}

/// Blank the plaintext device_key in the DB after it's been moved to the OS
/// secure store. Keeps the row (device_uuid lives here too); writes a zero-length
/// blob so the NOT NULL constraint holds.
pub async fn scrub_device_key(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query("UPDATE device_config SET device_key = ? WHERE id = 1")
        .bind(Vec::<u8>::new())
        .execute(pool)
        .await?;
    Ok(())
}

/// Load the stable device UUID used to mark note origin. Distinct from device_key:
/// this is a plaintext identifier that travels with transferred notes so the
/// receiver can recognize updates to previously-received notes. Generated on
/// first access and persisted in device_config.
pub async fn get_or_create_device_uuid(pool: &SqlitePool) -> anyhow::Result<String> {
    if let Some(row) = sqlx::query("SELECT device_uuid FROM device_config WHERE id = 1")
        .fetch_optional(pool)
        .await?
    {
        let existing: Option<String> = row.get("device_uuid");
        if let Some(uuid) = existing {
            if !uuid.is_empty() {
                return Ok(uuid);
            }
        }
    }
    let uuid = Uuid::new_v4().to_string();
    // device_config row is guaranteed to exist by the time this is called
    // (get_or_create_device_key runs first at startup).
    sqlx::query("UPDATE device_config SET device_uuid = ? WHERE id = 1")
        .bind(&uuid)
        .execute(pool)
        .await?;
    Ok(uuid)
}

/// Backfill origin_device_id / origin_note_id for rows created before migration 0010.
/// Pre-existing notes are local-only so we map origin_device_id = this_device and
/// origin_note_id = id. Safe to call repeatedly — only touches NULL rows.
pub async fn backfill_note_origins(pool: &SqlitePool, device_uuid: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE notes SET origin_device_id = ?, origin_note_id = id \
         WHERE origin_device_id IS NULL OR origin_note_id IS NULL",
    )
    .bind(device_uuid)
    .execute(pool)
    .await?;
    Ok(())
}

// ----- Device settings -----

pub async fn device_setting_get(pool: &SqlitePool, key: &str) -> anyhow::Result<Option<String>> {
    let row = sqlx::query("SELECT value FROM device_settings WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.get("value")))
}

pub async fn device_setting_set(pool: &SqlitePool, key: &str, value: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO device_settings (key, value) VALUES (?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

// ----- Notes -----

pub struct NoteRow {
    pub id: String,
    pub kind: String,
    pub title_nonce: Vec<u8>,
    pub title_ct: Vec<u8>,
    pub nonce: Vec<u8>,
    pub content_ct: Vec<u8>,
    pub note_salt: Option<Vec<u8>>,
    pub note_nonce: Option<Vec<u8>>,
    pub created_at: i64,
    pub updated_at: i64,
    pub tags: String,
    pub content_hint: Option<String>,
    pub pinned: bool,
    pub bg_color: Option<String>,
    pub bg_image: Option<String>,
    pub show_preview: bool,
    pub preview_text: Option<String>,
    pub origin_device_id: String,
    pub origin_note_id: String,
    /// Optional recovery-code wrap of the vault ciphertext (migration 0011).
    pub folder_id: Option<String>,
    pub sort_order: i64,
    pub rc_salt: Option<Vec<u8>>,
    pub rc_nonce: Option<Vec<u8>>,
    pub rc_ct: Option<Vec<u8>>,
}

fn row_to_note(r: sqlx::sqlite::SqliteRow) -> NoteRow {
    let origin_device_id: Option<String> = r.get("origin_device_id");
    let origin_note_id: Option<String> = r.get("origin_note_id");
    let id: String = r.get("id");
    NoteRow {
        kind: r.get("kind"),
        title_nonce: r.get("title_nonce"),
        title_ct: r.get("title_ct"),
        nonce: r.get("nonce"),
        content_ct: r.get("content_ct"),
        note_salt: r.get("note_salt"),
        note_nonce: r.get("note_nonce"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        tags: r.get("tags"),
        content_hint: r.get("content_hint"),
        pinned: { let v: i32 = r.get("pinned"); v != 0 },
        bg_color: r.get("bg_color"),
        bg_image: r.get("bg_image"),
        show_preview: { let v: i32 = r.get("show_preview"); v != 0 },
        preview_text: r.get("preview_text"),
        origin_device_id: origin_device_id.unwrap_or_else(|| String::new()),
        origin_note_id: origin_note_id.unwrap_or_else(|| id.clone()),
        folder_id: r.get("folder_id"),
        sort_order: r.get("sort_order"),
        rc_salt: r.get("rc_salt"),
        rc_nonce: r.get("rc_nonce"),
        rc_ct: r.get("rc_ct"),
        id,
    }
}

const SELECT_COLS: &str =
    "id, kind, title_nonce, title_ct, nonce, content_ct, note_salt, note_nonce, created_at, updated_at, tags, content_hint, pinned, bg_color, bg_image, show_preview, preview_text, origin_device_id, origin_note_id, rc_salt, rc_nonce, rc_ct, folder_id, sort_order";

/// The list view decrypts only the title and tags, so the body ciphertext and
/// the recovery wrap are pure read amplification — they scale with note size
/// and get dropped on the floor. Fetch neither.
const LIST_COLS: &str =
    "id, kind, title_nonce, title_ct, note_salt, created_at, updated_at, tags, content_hint, pinned, bg_color, show_preview, preview_text, origin_device_id, origin_note_id, folder_id, sort_order";

/// Maps a `LIST_COLS` row. The columns the list never reads are left empty;
/// only `note_list_page` may use this.
fn row_to_list_note(r: sqlx::sqlite::SqliteRow) -> NoteRow {
    let folder_id: Option<String> = r.get("folder_id");
    let sort_order: i64 = r.get("sort_order");
    let origin_device_id: Option<String> = r.get("origin_device_id");
    let origin_note_id: Option<String> = r.get("origin_note_id");
    let id: String = r.get("id");
    NoteRow {
        folder_id,
        sort_order,
        kind: r.get("kind"),
        title_nonce: r.get("title_nonce"),
        title_ct: r.get("title_ct"),
        nonce: Vec::new(),
        content_ct: Vec::new(),
        note_salt: r.get("note_salt"),
        note_nonce: None,
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
        tags: r.get("tags"),
        content_hint: r.get("content_hint"),
        pinned: { let v: i32 = r.get("pinned"); v != 0 },
        bg_color: r.get("bg_color"),
        // Not selected. A background is a base64 data URI that dwarfs everything
        // else in the row — measured at 143x the size of ALL note bodies put
        // together — and the list re-runs on every save, pin and delete. Fetched
        // once via `note_bg_images` and cached instead. See [`row_to_list_note`].
        bg_image: None,
        show_preview: { let v: i32 = r.get("show_preview"); v != 0 },
        preview_text: r.get("preview_text"),
        origin_device_id: origin_device_id.unwrap_or_else(|| String::new()),
        origin_note_id: origin_note_id.unwrap_or_else(|| id.clone()),
        rc_salt: None,
        rc_nonce: None,
        rc_ct: None,
        id,
    }
}

pub async fn note_insert(pool: &SqlitePool, row: &NoteRow) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO notes (id, kind, title_nonce, title_ct, nonce, content_ct, note_salt, note_nonce, created_at, updated_at, tags, content_hint, pinned, bg_color, bg_image, show_preview, preview_text, origin_device_id, origin_note_id, rc_salt, rc_nonce, rc_ct) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.kind)
    .bind(&row.title_nonce)
    .bind(&row.title_ct)
    .bind(&row.nonce)
    .bind(&row.content_ct)
    .bind(&row.note_salt)
    .bind(&row.note_nonce)
    .bind(row.created_at)
    .bind(row.updated_at)
    .bind(&row.tags)
    .bind(&row.content_hint)
    .bind(row.pinned as i32)
    .bind(&row.bg_color)
    .bind(&row.bg_image)
    .bind(row.show_preview as i32)
    .bind(&row.preview_text)
    .bind(&row.origin_device_id)
    .bind(&row.origin_note_id)
    .bind(&row.rc_salt)
    .bind(&row.rc_nonce)
    .bind(&row.rc_ct)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn note_update(pool: &SqlitePool, row: &NoteRow) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE notes SET kind=?, title_nonce=?, title_ct=?, nonce=?, content_ct=?, note_salt=?, note_nonce=?, updated_at=?, tags=?, content_hint=?, pinned=?, bg_color=?, bg_image=?, show_preview=?, preview_text=?, origin_device_id=?, origin_note_id=?, rc_salt=?, rc_nonce=?, rc_ct=? WHERE id=?",
    )
    .bind(&row.kind)
    .bind(&row.title_nonce)
    .bind(&row.title_ct)
    .bind(&row.nonce)
    .bind(&row.content_ct)
    .bind(&row.note_salt)
    .bind(&row.note_nonce)
    .bind(row.updated_at)
    .bind(&row.tags)
    .bind(&row.content_hint)
    .bind(row.pinned as i32)
    .bind(&row.bg_color)
    .bind(&row.bg_image)
    .bind(row.show_preview as i32)
    .bind(&row.preview_text)
    .bind(&row.origin_device_id)
    .bind(&row.origin_note_id)
    .bind(&row.rc_salt)
    .bind(&row.rc_nonce)
    .bind(&row.rc_ct)
    .bind(&row.id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Find a note by its origin identity (origin_device_id, origin_note_id).
/// Used by transfer/import to detect "same note, already received before".
pub async fn note_find_by_origin(
    pool: &SqlitePool,
    origin_device_id: &str,
    origin_note_id: &str,
) -> anyhow::Result<Option<NoteRow>> {
    let row = sqlx::query(&format!(
        "SELECT {SELECT_COLS} FROM notes WHERE origin_device_id = ? AND origin_note_id = ?"
    ))
    .bind(origin_device_id)
    .bind(origin_note_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(row_to_note))
}

pub async fn note_delete(pool: &SqlitePool, id: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM notes WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn note_pin(pool: &SqlitePool, id: &str, pinned: bool) -> anyhow::Result<()> {
    sqlx::query("UPDATE notes SET pinned = ? WHERE id = ?")
        .bind(pinned as i32)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Write an explicit order for one level. Positions come from the sequence, so
/// the caller sends the ids as the user arranged them and never computes indices.
pub async fn notes_set_order(pool: &SqlitePool, ids: &[String]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE notes SET sort_order = ? WHERE id = ?")
            .bind(i as i64)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn note_get(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<NoteRow>> {
    let row = sqlx::query(&format!("SELECT {SELECT_COLS} FROM notes WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_note))
}

// ----- Drafts -----

/// A note's unsaved edits. Encrypted like note content; see 0013_note_drafts.sql.
pub struct DraftRow {
    pub nonce: Vec<u8>,
    pub ct: Vec<u8>,
    pub updated_at: i64,
}

pub async fn draft_upsert(
    pool: &SqlitePool,
    note_id: &str,
    nonce: &[u8],
    ct: &[u8],
    updated_at: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO note_drafts (note_id, nonce, ct, updated_at) VALUES (?, ?, ?, ?) \
         ON CONFLICT(note_id) DO UPDATE SET nonce = excluded.nonce, ct = excluded.ct, \
         updated_at = excluded.updated_at",
    )
    .bind(note_id)
    .bind(nonce)
    .bind(ct)
    .bind(updated_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn draft_get(pool: &SqlitePool, note_id: &str) -> anyhow::Result<Option<DraftRow>> {
    let row = sqlx::query("SELECT nonce, ct, updated_at FROM note_drafts WHERE note_id = ?")
        .bind(note_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| DraftRow {
        nonce: r.get("nonce"),
        ct: r.get("ct"),
        updated_at: r.get("updated_at"),
    }))
}

pub async fn draft_delete(pool: &SqlitePool, note_id: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM note_drafts WHERE note_id = ?")
        .bind(note_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ----- Device identity -----

pub struct DeviceIdentityRow {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

pub async fn device_identity_get(pool: &SqlitePool) -> anyhow::Result<Option<DeviceIdentityRow>> {
    let row = sqlx::query("SELECT cert_der, key_der FROM device_identity WHERE id = 1")
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| DeviceIdentityRow {
        cert_der: r.get("cert_der"),
        key_der: r.get("key_der"),
    }))
}

pub async fn device_identity_insert(
    pool: &SqlitePool,
    cert_der: &[u8],
    key_der: &[u8],
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO device_identity (id, cert_der, key_der) VALUES (1, ?, ?)")
        .bind(cert_der)
        .bind(key_der)
        .execute(pool)
        .await?;
    Ok(())
}

/// Overwrite the stored `key_der` in place, leaving `cert_der` untouched.
/// Used to re-encrypt a legacy plaintext key the first time it's read (K7).
pub async fn device_identity_update_key(pool: &SqlitePool, key_der: &[u8]) -> anyhow::Result<()> {
    sqlx::query("UPDATE device_identity SET key_der = ? WHERE id = 1")
        .bind(key_der)
        .execute(pool)
        .await?;
    Ok(())
}

// ----- Known peers (TOFU) -----

#[allow(dead_code)]
pub struct KnownPeerRow {
    pub peer_id: String,
    pub fingerprint: Vec<u8>,
    pub first_seen: i64,
    pub last_seen: i64,
}

#[allow(dead_code)]
pub async fn known_peer_get(
    pool: &SqlitePool,
    peer_id: &str,
) -> anyhow::Result<Option<KnownPeerRow>> {
    let row = sqlx::query(
        "SELECT peer_id, fingerprint, first_seen, last_seen FROM known_peers WHERE peer_id = ?",
    )
    .bind(peer_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| KnownPeerRow {
        peer_id: r.get("peer_id"),
        fingerprint: r.get("fingerprint"),
        first_seen: r.get("first_seen"),
        last_seen: r.get("last_seen"),
    }))
}

pub async fn known_peer_upsert(
    pool: &SqlitePool,
    peer_id: &str,
    fingerprint: &[u8],
    now: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO known_peers (peer_id, fingerprint, first_seen, last_seen) VALUES (?, ?, ?, ?)
         ON CONFLICT(peer_id) DO UPDATE SET fingerprint = excluded.fingerprint, last_seen = excluded.last_seen",
    )
    .bind(peer_id)
    .bind(fingerprint)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn known_peers_list(pool: &SqlitePool) -> anyhow::Result<Vec<KnownPeerRow>> {
    let rows = sqlx::query(
        "SELECT peer_id, fingerprint, first_seen, last_seen FROM known_peers ORDER BY last_seen DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| KnownPeerRow {
            peer_id: r.get("peer_id"),
            fingerprint: r.get("fingerprint"),
            first_seen: r.get("first_seen"),
            last_seen: r.get("last_seen"),
        })
        .collect())
}

pub struct KnownPeerHistoryRow {
    pub peer_id: String,
    pub display_name: Option<String>,
    pub last_transfer_at: Option<i64>,
}

fn cap_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// Upsert a peer's display name and last_transfer_at without touching the TOFU fingerprint.
pub async fn known_peer_record_transfer(
    pool: &SqlitePool,
    peer_id: &str,
    display_name: &str,
    now: i64,
) -> anyhow::Result<()> {
    let peer_id = cap_str(peer_id, 128);
    let display_name = cap_str(display_name, 128);
    sqlx::query(
        "INSERT INTO known_peers (peer_id, fingerprint, display_name, first_seen, last_seen, last_transfer_at)
         VALUES (?, X'', ?, ?, ?, ?)
         ON CONFLICT(peer_id) DO UPDATE SET
           display_name = excluded.display_name,
           last_seen = excluded.last_seen,
           last_transfer_at = excluded.last_transfer_at",
    )
    .bind(peer_id)
    .bind(display_name)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn known_peers_list_history(pool: &SqlitePool) -> anyhow::Result<Vec<KnownPeerHistoryRow>> {
    let rows = sqlx::query(
        "SELECT peer_id, display_name, last_transfer_at FROM known_peers \
         WHERE last_transfer_at IS NOT NULL ORDER BY last_transfer_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| KnownPeerHistoryRow {
            peer_id: r.get("peer_id"),
            display_name: r.get("display_name"),
            last_transfer_at: r.get("last_transfer_at"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;

    async fn pool() -> SqlitePool {
        init_pool(":memory:").await.unwrap()
    }

    // ----- cap_str -----

    #[test]
    fn cap_str_empty_is_unchanged() {
        assert_eq!(cap_str("", 10), "");
    }

    #[test]
    fn cap_str_within_limit_is_unchanged() {
        assert_eq!(cap_str("hello", 10), "hello");
    }

    #[test]
    fn cap_str_exactly_at_limit_is_unchanged() {
        assert_eq!(cap_str("hello", 5), "hello");
    }

    #[test]
    fn cap_str_over_limit_truncates() {
        assert_eq!(cap_str("hello world", 5), "hello");
    }

    #[test]
    fn cap_str_unicode_does_not_split_char() {
        // "é" is 2 bytes; cap at 1 char should give "é", not a broken byte slice
        let s = "éàü";
        let capped = cap_str(s, 2);
        assert_eq!(capped, "éà");
        assert!(std::str::from_utf8(capped.as_bytes()).is_ok());
    }

    // ----- known_peer_record_transfer -----

    #[tokio::test]
    async fn record_transfer_inserts_new_peer() {
        let pool = pool().await;
        known_peer_record_transfer(&pool, "192.168.1.5", "Alice's phone", 1000)
            .await
            .unwrap();
        let rows = known_peers_list_history(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].peer_id, "192.168.1.5");
        assert_eq!(rows[0].display_name.as_deref(), Some("Alice's phone"));
        assert_eq!(rows[0].last_transfer_at, Some(1000));
    }

    #[tokio::test]
    async fn record_transfer_upsert_updates_display_name_and_timestamp() {
        let pool = pool().await;
        known_peer_record_transfer(&pool, "192.168.1.5", "Old name", 1000).await.unwrap();
        known_peer_record_transfer(&pool, "192.168.1.5", "New name", 2000).await.unwrap();
        let rows = known_peers_list_history(&pool).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].display_name.as_deref(), Some("New name"));
        assert_eq!(rows[0].last_transfer_at, Some(2000));
    }

    #[tokio::test]
    async fn record_transfer_does_not_overwrite_fingerprint() {
        let pool = pool().await;
        let fp = [0xabu8; 32];
        known_peer_upsert(&pool, "192.168.1.5", &fp, 500).await.unwrap();
        // record_transfer must not zero out the fingerprint
        known_peer_record_transfer(&pool, "192.168.1.5", "Alice", 1000).await.unwrap();
        let row = known_peer_get(&pool, "192.168.1.5").await.unwrap().unwrap();
        assert_eq!(row.fingerprint, fp);
    }

    // ----- known_peers_list_history -----

    #[tokio::test]
    async fn list_history_empty_when_no_transfers() {
        let pool = pool().await;
        let rows = known_peers_list_history(&pool).await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn list_history_excludes_tofu_only_peers() {
        let pool = pool().await;
        // Insert a TOFU-only peer (fingerprint set, no transfer)
        known_peer_upsert(&pool, "192.168.1.10", &[0u8; 32], 100).await.unwrap();
        let rows = known_peers_list_history(&pool).await.unwrap();
        assert!(rows.is_empty(), "TOFU-only peer should not appear in transfer history");
    }

    #[tokio::test]
    async fn list_history_ordered_by_last_transfer_at_desc() {
        let pool = pool().await;
        known_peer_record_transfer(&pool, "192.168.1.1", "A", 1000).await.unwrap();
        known_peer_record_transfer(&pool, "192.168.1.2", "B", 3000).await.unwrap();
        known_peer_record_transfer(&pool, "192.168.1.3", "C", 2000).await.unwrap();
        let rows = known_peers_list_history(&pool).await.unwrap();
        let timestamps: Vec<_> = rows.iter().map(|r| r.last_transfer_at.unwrap()).collect();
        assert_eq!(timestamps, vec![3000, 2000, 1000]);
    }
}

/// Lists every note, newest-first. Used by export/import, which must see the
/// full set — do not add pagination here; see `note_list_page` for the
/// bounded variant used by the frontend list view (K14).
pub async fn note_list(pool: &SqlitePool) -> anyhow::Result<Vec<NoteRow>> {
    let rows = sqlx::query(&format!(
        "SELECT {SELECT_COLS} FROM notes ORDER BY updated_at DESC"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_note).collect())
}

/// Lists notes, bounded by `limit`/`offset` (K14) so the frontend list view
/// can't force decrypting every note in the database at once.
///
/// Ordered `pinned DESC` first, then newest: pinning is the user saying "this
/// one matters", so a pinned note must never be the one that falls outside the
/// window. Before this, pinning a note older than the newest 500 made it vanish
/// from the Pinned section entirely.
///
/// Returned rows carry no body ciphertext — see [`row_to_list_note`].
pub async fn note_list_page(pool: &SqlitePool, limit: i64, offset: i64) -> anyhow::Result<Vec<NoteRow>> {
    let rows = sqlx::query(&format!(
        "SELECT {LIST_COLS} FROM notes ORDER BY pinned DESC, updated_at DESC LIMIT ? OFFSET ?"
    ))
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_list_note).collect())
}

/// Every note that has a background image, as `(id, data_uri)`.
///
/// Deliberately separate from the list query: backgrounds are large and change
/// rarely, so they are fetched once and cached rather than re-serialised through
/// IPC every time a note is saved, pinned or deleted.
pub async fn note_bg_images(pool: &SqlitePool) -> anyhow::Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT id, bg_image FROM notes WHERE bg_image IS NOT NULL AND bg_image <> ''")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get("id"), r.get("bg_image")))
        .collect())
}

/// Total notes, so the list can say how many it is NOT showing. Counting is
/// cheap — no decryption, no row payload.
pub async fn note_count(pool: &SqlitePool) -> anyhow::Result<i64> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM notes")
        .fetch_one(pool)
        .await?;
    Ok(row.get("n"))
}
