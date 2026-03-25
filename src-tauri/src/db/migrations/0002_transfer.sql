-- Device's own TLS identity (one row)
CREATE TABLE IF NOT EXISTS device_identity (
    id       INTEGER PRIMARY KEY CHECK (id = 1),
    cert_der BLOB NOT NULL,
    key_der  BLOB NOT NULL
);

-- TOFU peer certificate fingerprints
CREATE TABLE IF NOT EXISTS known_peers (
    peer_id      TEXT PRIMARY KEY,   -- mDNS instance name or BLE device address
    fingerprint  BLOB NOT NULL,       -- SHA-256 of peer's cert DER (32 bytes)
    first_seen   INTEGER NOT NULL,
    last_seen    INTEGER NOT NULL
);
