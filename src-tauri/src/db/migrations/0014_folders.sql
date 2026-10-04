-- Folders: nested, one folder per note.
--
-- `name_ct` is base64(nonce||ct) with the folder id as AAD, the same framing
-- tags use — a folder called "Bank accounts" leaks as much as a tag would, so
-- it is not stored in the clear.
CREATE TABLE IF NOT EXISTS folders (
    id         TEXT PRIMARY KEY NOT NULL,
    parent_id  TEXT REFERENCES folders(id) ON DELETE CASCADE,
    name_ct    TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_folders_parent ON folders(parent_id);

-- ON DELETE SET NULL, never CASCADE: deleting a folder must return its notes to
-- the root, never destroy them. Subfolders do cascade (above).
ALTER TABLE notes ADD COLUMN folder_id TEXT REFERENCES folders(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_notes_folder ON notes(folder_id);
