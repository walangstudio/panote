-- Stable device identifier for note origin tracking (separate from device_key,
-- which is the encryption key). Populated at startup if NULL.
ALTER TABLE device_config ADD COLUMN device_uuid TEXT;

-- Every note records which device originally created it and what its local id
-- was on that device. For notes created here, origin_device_id = this device's
-- uuid and origin_note_id = id. For notes received via transfer or import,
-- both come from the sender. This lets us detect "same note, seen before"
-- across transfers and imports so we can update rather than duplicate.
--
-- Both columns are logically NOT NULL but SQLite ALTER TABLE can't add NOT
-- NULL without a default on existing rows — startup code backfills them
-- before any command runs.
ALTER TABLE notes ADD COLUMN origin_device_id TEXT;
ALTER TABLE notes ADD COLUMN origin_note_id TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS idx_notes_origin
  ON notes(origin_device_id, origin_note_id);
