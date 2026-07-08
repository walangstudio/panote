-- Optional per-note recovery code. When a note is protected and the user opts
-- in to recovery, the same vault ciphertext is wrapped a second time under a
-- key derived from a high-entropy recovery code (Argon2id), so forgetting the
-- password isn't fatal. Only the wrap is stored here — never the code itself.
-- All nullable: notes without recovery leave these NULL.
ALTER TABLE notes ADD COLUMN rc_salt BLOB;
ALTER TABLE notes ADD COLUMN rc_nonce BLOB;
ALTER TABLE notes ADD COLUMN rc_ct BLOB;
