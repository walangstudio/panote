use sha2::{Digest, Sha384};
use sqlx::{migrate::Migrator, sqlite::SqliteConnectOptions, Row, SqlitePool};
use std::str::FromStr;

pub mod queries;

static MIGRATOR: Migrator = sqlx::migrate!("./src/db/migrations");

pub async fn init_pool(db_path: &str) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite:{db_path}"))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        // Explicit rather than relying on sqlx's default: folder deletion leans on
        // ON DELETE CASCADE for subfolders and ON DELETE SET NULL for notes, and
        // with the pragma off SQLite silently ignores both — which would orphan
        // subfolders and, worse, leave notes pointing at a folder that is gone.
        .foreign_keys(true);

    let pool = SqlitePool::connect_with(opts).await?;
    repair_crlf_checksums(&pool).await?;
    MIGRATOR.run(&pool).await?;
    Ok(pool)
}

/// sqlx checksums a migration's raw bytes. Builds made from a CRLF checkout
/// recorded the CRLF variant, and every LF build since refuses those databases
/// as "previously applied but modified". Same SQL, so re-stamp the canonical sum.
async fn repair_crlf_checksums(pool: &SqlitePool) -> anyhow::Result<()> {
    let has_table = sqlx::query(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_optional(pool)
    .await?
    .is_some();
    if !has_table {
        return Ok(());
    }
    for m in MIGRATOR.iter() {
        let crlf = Sha384::digest(m.sql.replace('\n', "\r\n").as_bytes());
        let Some(row) = sqlx::query("SELECT checksum FROM _sqlx_migrations WHERE version = ?")
            .bind(m.version)
            .fetch_optional(pool)
            .await?
        else {
            continue;
        };
        let stored: Vec<u8> = row.get("checksum");
        if stored.as_slice() == crlf.as_slice() {
            sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = ?")
                .bind(m.checksum.as_ref())
                .bind(m.version)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reopens_a_database_stamped_by_a_crlf_build() {
        let path = std::env::temp_dir().join(format!("panote-crlf-{}.db", std::process::id()));
        let path_str = path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);

        let pool = init_pool(&path_str).await.unwrap();
        let m = MIGRATOR.iter().find(|m| m.version == 8).unwrap();
        let crlf = Sha384::digest(m.sql.replace('\n', "\r\n").as_bytes()).to_vec();
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = 8")
            .bind(&crlf)
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        let pool = init_pool(&path_str)
            .await
            .expect("CRLF-stamped database must reopen");
        let stored: Vec<u8> =
            sqlx::query("SELECT checksum FROM _sqlx_migrations WHERE version = 8")
                .fetch_one(&pool)
                .await
                .unwrap()
                .get("checksum");
        assert_eq!(stored, m.checksum.to_vec());
        pool.close().await;
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn still_rejects_a_genuinely_modified_migration() {
        let path = std::env::temp_dir().join(format!("panote-modified-{}.db", std::process::id()));
        let path_str = path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);

        let pool = init_pool(&path_str).await.unwrap();
        sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 8")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;

        assert!(init_pool(&path_str).await.is_err());
        let _ = std::fs::remove_file(&path);
    }
}
