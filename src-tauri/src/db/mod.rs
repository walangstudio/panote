use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

pub mod queries;

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
    sqlx::migrate!("./src/db/migrations").run(&pool).await?;
    Ok(pool)
}
