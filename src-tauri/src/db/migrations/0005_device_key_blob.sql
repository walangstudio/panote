-- Replace app_uuid text derivation with a stored random key blob.
-- Simpler and cross-platform (machine-uid crate doesn't support Android).
DROP TABLE IF EXISTS device_config;

CREATE TABLE IF NOT EXISTS device_config (
  id         INTEGER PRIMARY KEY CHECK (id = 1),
  device_key BLOB NOT NULL  -- random 32-byte key, generated once on first launch
);
