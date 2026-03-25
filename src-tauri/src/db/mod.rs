use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use std::str::FromStr;

pub mod queries;

pub async fn init_pool(db_path: &str) -> anyhow::Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(&format!("sqlite:{db_path}"))?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);

    let pool = SqlitePool::connect_with(opts).await?;
    sqlx::migrate!("./src/db/migrations").run(&pool).await?;
    Ok(pool)
}
