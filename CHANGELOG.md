# Changelog

## [0.1.0] - 2026-03-25

Initial release.

### Notes

- Five note types: plain text, Markdown (with preview), checklist, code (with syntax highlighting), Kanban board
- Kanban columns and cards can be reordered by dragging the handle; works on both desktop (mouse) and Android (touch)
- Tags with comma or Enter to add, click × to remove
- Notes list with search by title or tag

### Storage

- Notes encrypted at rest with a random 32-byte device key generated on first launch (ChaCha20-Poly1305)
- Key stored in the local SQLite database; never leaves the device
- Database stored in the OS app data directory

### Transfer

- LAN peer discovery via mDNS (`_panote._tcp.local.`)
- Note transfer requires both sides to enter the same passphrase
- Transfer payload encrypted independently of the TLS tunnel (Argon2id key derivation + ChaCha20-Poly1305)
- Wrong passphrase produces a decryption error; the note is not imported
- TLS 1.3 transport with self-signed certificates and TOFU fingerprint pinning

### Platform

- Desktop: Windows, macOS, Linux
- Mobile: Android (ARM and x86_64)
- Collapsible sidebar on narrow screens
