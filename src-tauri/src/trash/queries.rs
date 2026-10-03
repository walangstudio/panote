//! Trash rows. Titles arrive encrypted; `commands` decrypts them.

use sqlx::{Row, SqlitePool};

pub struct TrashRow {
    pub id: String,
    pub kind: String,
    pub title_nonce: Vec<u8>,
    pub title_ct: Vec<u8>,
    pub note_salt: Option<Vec<u8>>,
    pub content_hint: Option<String>,
    pub deleted_at: i64,
}

pub async fn trash(pool: &SqlitePool, ids: &[String], now: i64) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for id in ids {
        sqlx::query("UPDATE notes SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL")
            .bind(now)
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Newest-deleted first. Only what the Trash view shows: no body, no preview.
pub async fn list(pool: &SqlitePool) -> anyhow::Result<Vec<TrashRow>> {
    let rows = sqlx::query(
        "SELECT id, kind, title_nonce, title_ct, note_salt, content_hint, deleted_at FROM notes \
         WHERE deleted_at IS NOT NULL ORDER BY deleted_at DESC, id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| TrashRow {
            id: r.get("id"),
            kind: r.get("kind"),
            title_nonce: r.get("title_nonce"),
            title_ct: r.get("title_ct"),
            note_salt: r.get("note_salt"),
            content_hint: r.get("content_hint"),
            deleted_at: r.get("deleted_at"),
        })
        .collect())
}

/// Back into the folder it was deleted from, or the root if that folder is gone.
/// Deleting a folder already nulls `folder_id` via ON DELETE SET NULL; the CASE
/// covers a database where that never ran.
/// Returns how many notes actually came out of Trash; live ids are left alone.
pub async fn restore(pool: &SqlitePool, ids: &[String]) -> anyhow::Result<u64> {
    let mut tx = pool.begin().await?;
    let mut restored = 0;
    for id in ids {
        restored += sqlx::query(
            "UPDATE notes SET deleted_at = NULL, \
             folder_id = CASE WHEN folder_id IN (SELECT id FROM folders) THEN folder_id END \
             WHERE id = ? AND deleted_at IS NOT NULL",
        )
        .bind(id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    }
    tx.commit().await?;
    Ok(restored)
}

/// Permanent. Only ever touches trashed rows, so a live note cannot be destroyed
/// by passing its id here.
pub async fn purge(pool: &SqlitePool, ids: &[String]) -> anyhow::Result<()> {
    let mut tx = pool.begin().await?;
    for id in ids {
        sqlx::query("DELETE FROM notes WHERE id = ? AND deleted_at IS NOT NULL")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn empty(pool: &SqlitePool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM notes WHERE deleted_at IS NOT NULL")
        .execute(pool)
        .await?;
    Ok(())
}

/// Everything trashed before `cutoff`, for the startup purge.
pub async fn purge_before(pool: &SqlitePool, cutoff: i64) -> anyhow::Result<u64> {
    let done = sqlx::query("DELETE FROM notes WHERE deleted_at < ?")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(done.rows_affected())
}
