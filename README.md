<div align="center">

<img src="static/icon.png" width="96" alt="Panote" />

# Panote

[![Version](https://img.shields.io/badge/version-0.1.0-blue?style=flat-square)](src-tauri/tauri.conf.json)
[![Rust](https://img.shields.io/badge/Rust-1.78%2B-orange?style=flat-square&logo=rust&logoColor=white)](https://rust-lang.org)
[![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00?style=flat-square&logo=svelte&logoColor=white)](https://svelte.dev)
[![License](https://img.shields.io/badge/License-MIT-22c55e?style=flat-square)](LICENSE)

</div>

---

A local-first note-taking app for desktop and Android, built with Tauri 2, Svelte 5, and Rust. Notes are stored encrypted on-device. Sharing a note with another device requires both sides to enter the same passphrase; nothing goes through a server.

Supported note types: Plain text, Markdown, Checklist, Code, Kanban.

---

## How transfer works

Both the sender and receiver open the Transfer screen. The sender picks a note and peer, types a passphrase, and sends. The receiver sees the incoming transfer and types the same passphrase to accept it. The note payload is encrypted with a key derived from that passphrase before it leaves the sender, so the TLS tunnel is not the only thing protecting it.

Peers are discovered automatically on the local network via mDNS. No pairing, no accounts, no cloud.

---

## Requirements

- Windows 10+, macOS 12+, or Linux (desktop)
- Android 7.0+ (mobile)
- Rust 1.78+
- Node.js 18+
- JDK 17+ with `JAVA_HOME` set (Android builds only)
- Android SDK and NDK (Android builds only; install via Android Studio or `sdkmanager`)

For Android builds, Windows also requires Developer Mode enabled (Settings > System > For developers).

---

## Development

```bash
git clone https://github.com/walangstudio/panote.git
cd panote
npm install
npm test            # run unit tests
npm run tauri dev
```

For Android (first time only, initializes the Android project):

```bash
npm run tauri android init
npm run tauri android dev
```

---

## Building

```bash
npm run tauri build
```

For Android:

```bash
npm run tauri android build
```

Installers are output to `src-tauri/target/release/bundle/`.

---

## Project structure

```
panote/
├── src/                        # Svelte frontend
│   ├── routes/
│   │   ├── +page.svelte        # Notes list
│   │   ├── note/[id]/          # Note editor
│   │   └── transfer/           # Peer discovery and transfer
│   └── lib/
│       ├── tauri.ts            # Tauri command bindings
│       ├── kanban.ts           # Kanban drag-and-drop logic
│       ├── stores/             # Svelte stores
│       └── components/         # Note type editors
└── src-tauri/                  # Rust backend
    └── src/
        ├── crypto/             # Encryption primitives, TLS, TOFU
        ├── db/                 # SQLite migrations and queries
        ├── notes/              # Note CRUD commands
        ├── transfer/           # LAN (mDNS + TLS) and BLE transport
        └── state.rs            # Shared app state
```

---

## Storage

Notes are encrypted at rest with a 32-byte key generated on first launch and stored in the local SQLite database. The database lives in the OS app data directory and is never synced anywhere.

Copying the database to a different device will not work; the key does not travel with the file.

---

## Security notes

- Transport uses TLS 1.3 with self-signed certificates and TOFU fingerprint pinning. A changed fingerprint on reconnect is rejected.
- Note payloads are additionally encrypted with a key derived from the shared passphrase (Argon2id + ChaCha20-Poly1305) before transmission. A wrong passphrase produces a decryption error, not garbage data.
- BLE transport is stubbed and not yet functional. The btleplug peripheral role is unsupported on Windows, and the feature is deferred to a future release.

---

## License

MIT
