-- Redesign vault to key-wrapping: vault_key is random, stored encrypted under
-- both password and recovery code. This allows password reset without data loss.
--
-- pw_salt/pw_nonce/pw_ct:  encrypt(vault_key, derive_key(password, pw_salt))
-- rc_salt/rc_nonce/rc_ct:  encrypt(vault_key, derive_key(recovery_code, rc_salt))

DROP TABLE IF EXISTS vault;

CREATE TABLE vault (
  id       INTEGER PRIMARY KEY CHECK (id = 1),
  pw_salt  BLOB NOT NULL,
  pw_nonce BLOB NOT NULL,
  pw_ct    BLOB NOT NULL,
  rc_salt  BLOB NOT NULL,
  rc_nonce BLOB NOT NULL,
  rc_ct    BLOB NOT NULL
);
