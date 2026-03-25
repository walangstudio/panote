-- Vault metadata: one row only
CREATE TABLE IF NOT EXISTS vault (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    salt        BLOB NOT NULL,       -- Argon2id salt (16 bytes)
    nonce_check BLOB NOT NULL,       -- nonce used for verifier ciphertext (12 bytes)
    check_ct    BLOB NOT NULL        -- ChaCha20-Poly1305 ciphertext of b"panote-ok"
);

-- Notes
CREATE TABLE IF NOT EXISTS notes (
    id          TEXT PRIMARY KEY,    -- UUID v4
    kind        TEXT NOT NULL,       -- 'markdown' | 'checklist' | 'code' | 'kanban'
    title_nonce BLOB NOT NULL,       -- nonce for title ciphertext
    title_ct    BLOB NOT NULL,       -- ChaCha20-Poly1305 encrypted title
    nonce       BLOB NOT NULL,       -- nonce for content ciphertext
    content_ct  BLOB NOT NULL,       -- ChaCha20-Poly1305 encrypted JSON content
    note_salt   BLOB,                -- Argon2id salt for per-note password (NULL if none)
    note_nonce  BLOB,                -- nonce for per-note encryption layer (NULL if none)
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    tags        TEXT NOT NULL DEFAULT '[]'  -- plaintext JSON array of tag strings
);

CREATE INDEX IF NOT EXISTS idx_notes_kind ON notes(kind);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC);
