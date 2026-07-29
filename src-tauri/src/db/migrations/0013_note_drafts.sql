-- Unsaved edits, held apart from the committed note.
--
-- Editing must never overwrite a note until the user explicitly saves, so
-- in-progress work lands here instead. A separate table, not extra columns on
-- `notes`, so a draft can never creep into the list query's column set.
--
-- Encrypted exactly like note content: ChaCha20-Poly1305 under the device key,
-- AAD-bound to the note id. A plaintext draft would defeat at-rest encryption
-- the same way the plaintext preview column does.
CREATE TABLE IF NOT EXISTS note_drafts (
    note_id    TEXT PRIMARY KEY NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    nonce      BLOB NOT NULL,
    ct         BLOB NOT NULL,
    updated_at INTEGER NOT NULL
);
