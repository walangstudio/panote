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
    }
}

const SELECT_COLS: &str =
    "id, kind, title_nonce, title_ct, nonce, content_ct, note_salt, note_nonce, created_at, updated_at, tags";

pub async fn note_insert(pool: &SqlitePool, row: &NoteRow) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO notes (id, kind, title_nonce, title_ct, nonce, content_ct, note_salt, note_nonce, created_at, updated_at, tags) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn note_update(pool: &SqlitePool, row: &NoteRow) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE notes SET title_nonce=?, title_ct=?, nonce=?, content_ct=?, note_salt=?, note_nonce=?, updated_at=?, tags=? WHERE id=?",
    )
    .bind(&row.title_nonce)
    .bind(&row.title_ct)
    .bind(&row.nonce)
    .bind(&row.content_ct)
    .bind(&row.note_salt)
    .bind(&row.note_nonce)
    .bind(row.updated_at)
    .bind(&row.tags)
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

pub async fn note_list(pool: &SqlitePool) -> anyhow::Result<Vec<NoteRow>> {
    let rows = sqlx::query(&format!(
        "SELECT {SELECT_COLS} FROM notes ORDER BY updated_at DESC"
    ))
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(row_to_note).collect())
}
