use rand::{rngs::OsRng, RngCore};
use sqlx::{Row, SqlitePool};

// ----- Device config -----

/// Load the device key, generating and persisting a new random one on first launch.
pub async fn get_or_create_device_key(pool: &SqlitePool) -> anyhow::Result<[u8; 32]> {
    if let Some(row) = sqlx::query("SELECT device_key FROM device_config WHERE id = 1")
        .fetch_optional(pool)
        .await?
    {
        let bytes: Vec<u8> = row.get("device_key");
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    sqlx::query("INSERT INTO device_config (id, device_key) VALUES (1, ?)")
        .bind(key.as_slice())
        .execute(pool)
        .await?;
    Ok(key)
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
}

fn row_to_note(r: sqlx::sqlite::SqliteRow) -> NoteRow {
    NoteRow {
        id: r.get("id"),
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
    }
}

const SELECT_COLS: &str =
    "id, kind, title_nonce, title_ct, nonce, content_ct, note_salt, note_nonce, created_at, updated_at, tags, content_hint, pinned, bg_color, bg_image, show_preview, preview_text";

pub async fn note_insert(pool: &SqlitePool, row: &NoteRow) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO notes (id, kind, title_nonce, title_ct, nonce, content_ct, note_salt, note_nonce, created_at, updated_at, tags, content_hint, pinned, bg_color, bg_image, show_preview, preview_text) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn note_update(pool: &SqlitePool, row: &NoteRow) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE notes SET kind=?, title_nonce=?, title_ct=?, nonce=?, content_ct=?, note_salt=?, note_nonce=?, updated_at=?, tags=?, content_hint=?, pinned=?, bg_color=?, bg_image=?, show_preview=?, preview_text=? WHERE id=?",
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
    .bind(&row.id)
    .execute(pool)
    .await?;
    Ok(())
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

pub async fn note_get(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<NoteRow>> {
    let row = sqlx::query(&format!("SELECT {SELECT_COLS} FROM notes WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(row_to_note))
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

pub async fn note_list(pool: &SqlitePool) -> anyhow::Result<Vec<NoteRow>> {
    let rows = sqlx::query(&format!(
        "SELECT {SELECT_COLS} FROM notes ORDER BY updated_at DESC"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_note).collect())
}
