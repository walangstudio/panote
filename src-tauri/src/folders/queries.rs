//! Folder rows. Names arrive encrypted; `commands` decrypts them.

use sqlx::{Row, SqlitePool};

pub struct FolderRow {
    pub id: String,
    pub parent_id: Option<String>,
    pub name_ct: String,
    pub created_at: i64,
    pub updated_at: i64,
}

fn to_row(r: sqlx::sqlite::SqliteRow) -> FolderRow {
    FolderRow {
        id: r.get("id"),
        parent_id: r.get("parent_id"),
        name_ct: r.get("name_ct"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

pub async fn insert(
    pool: &SqlitePool,
    id: &str,
    parent_id: Option<&str>,
    name_ct: &str,
    now: i64,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO folders (id, parent_id, name_ct, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(parent_id)
    .bind(name_ct)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<FolderRow>> {
    let row = sqlx::query("SELECT id, parent_id, name_ct, created_at, updated_at FROM folders WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(to_row))
}

pub async fn list(pool: &SqlitePool) -> anyhow::Result<Vec<FolderRow>> {
    let rows = sqlx::query(
        "SELECT id, parent_id, name_ct, created_at, updated_at FROM folders ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(to_row).collect())
}

pub async fn rename(pool: &SqlitePool, id: &str, name_ct: &str, now: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE folders SET name_ct = ?, updated_at = ? WHERE id = ?")
        .bind(name_ct)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_parent(
    pool: &SqlitePool,
    id: &str,
    parent_id: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE folders SET parent_id = ?, updated_at = ? WHERE id = ?")
        .bind(parent_id)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Subfolders cascade; notes fall back to the root via ON DELETE SET NULL.
/// Both depend on `foreign_keys` being on - see `db::init_pool`.
pub async fn delete(pool: &SqlitePool, id: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM folders WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_note_folder(
    pool: &SqlitePool,
    note_id: &str,
    folder_id: Option<&str>,
    now: i64,
) -> anyhow::Result<()> {
    sqlx::query("UPDATE notes SET folder_id = ?, updated_at = ? WHERE id = ?")
        .bind(folder_id)
        .bind(now)
        .bind(note_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn note_folder(pool: &SqlitePool, note_id: &str) -> anyhow::Result<Option<String>> {
    let row = sqlx::query("SELECT folder_id FROM notes WHERE id = ?")
        .bind(note_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|r| r.get::<Option<String>, _>("folder_id")))
}

/// Note counts per folder, for the tree. Notes at the root are not counted.
pub async fn note_counts(pool: &SqlitePool) -> anyhow::Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        "SELECT folder_id, COUNT(*) AS n FROM notes WHERE folder_id IS NOT NULL GROUP BY folder_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.get::<String, _>("folder_id"), r.get::<i64, _>("n")))
        .collect())
}

/// Every note in `folder_id`, and in its subfolders, for a folder send.
pub async fn note_ids_in_subtree(
    pool: &SqlitePool,
    folder_id: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        "WITH RECURSIVE sub(id) AS (
             SELECT ?
             UNION ALL
             SELECT f.id FROM folders f JOIN sub ON f.parent_id = sub.id
         )
         SELECT n.id FROM notes n JOIN sub ON n.folder_id = sub.id
         ORDER BY n.updated_at DESC",
    )
    .bind(folder_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.get::<String, _>("id")).collect())
}
