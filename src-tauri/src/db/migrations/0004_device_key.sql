-- Remove vault-based auth. Notes now use a device-derived key (transparent to user).
DROP TABLE IF EXISTS vault;

CREATE TABLE IF NOT EXISTS device_config (
  id       INTEGER PRIMARY KEY CHECK (id = 1),
  app_uuid TEXT NOT NULL  -- random UUID, combined with machine UID to derive device_key
);
