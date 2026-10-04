-- A password-protected note seals its title under the note key, like its body.
-- The nonce for the title's password layer, mirroring note_nonce. NULL when the
-- title sits under the device key only: unprotected notes, and notes protected
-- before this change until they are next unlocked.
ALTER TABLE notes ADD COLUMN title_note_nonce BLOB;
