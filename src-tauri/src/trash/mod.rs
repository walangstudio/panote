//! Trash: deleting a note sets `deleted_at` instead of removing the row, so the
//! delete can be undone until the note is purged.

pub mod commands;
pub mod queries;

#[cfg(test)]
mod tests;

/// How long a trashed note is kept before the startup purge removes it.
pub const RETENTION_SECS: i64 = 30 * 24 * 60 * 60;
