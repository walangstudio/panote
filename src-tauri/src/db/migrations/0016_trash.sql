-- Trash: deleting a note stamps it instead of removing the row, so a delete can
-- be undone. NULL is a live note. Every list, count and export filters on it.
--
-- Rows stamped more than 30 days ago are purged at startup. The ciphertext is
-- untouched while trashed, so a protected note stays protected in Trash.
ALTER TABLE notes ADD COLUMN deleted_at INTEGER;

-- Partial: only trashed rows are indexed, which is all the Trash view and the
-- purge ever look up.
CREATE INDEX IF NOT EXISTS idx_notes_deleted ON notes(deleted_at) WHERE deleted_at IS NOT NULL;
