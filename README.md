<div align="center">

<img src="static/icon.png" width="96" alt="Panote" />

# Panote

[![Version](https://img.shields.io/badge/version-0.2.0-blue?style=flat-square)](src-tauri/tauri.conf.json)
[![Rust](https://img.shields.io/badge/Rust-1.78%2B-orange?style=flat-square&logo=rust&logoColor=white)](https://rust-lang.org)
[![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00?style=flat-square&logo=svelte&logoColor=white)](https://svelte.dev)
[![License](https://img.shields.io/badge/License-MIT-22c55e?style=flat-square)](LICENSE)

</div>

---

A local-first note-taking app for desktop and Android, built with Tauri 2, Svelte 5, and Rust. Notes are stored encrypted on-device. Transferring a note to another device uses a one-time pairing code; nothing goes through a server.

---

## Features

**Note types**

- **Plain text** — simple textarea, no formatting
- **Markdown** — editor with live preview; preview is the default view when opening an existing note
- **Checklist** — nested check items with keyboard navigation
- **Code** — split-pane editor with syntax highlighting (Rust, TypeScript, JavaScript, Python, Go, Bash, SQL, JSON, HTML, CSS)
- **Kanban** — columns and cards, drag to reorder via handle; works on desktop (mouse) and Android (touch)

**Notes list**

- Search by title or tag
- Tags shown on each card; filter updates instantly as you type
- Note type shown with an icon
- Multi-select mode — tap **Select**, check notes, then **Send selected** to transfer multiple notes at once

**Tags**

- Add multiple tags by separating with commas or pressing Enter
- Tapping away from the tag input or saving also commits the current text (fixes mobile tag loss)
- Remove individual tags with ×

**Transfer**

- LAN peer discovery via mDNS and UDP broadcast beacon (works across WiFi/Ethernet boundaries where mDNS multicast is filtered)
- Send from any note using the **···** menu in the note header, or from the notes list in multi-select mode
- Sender generates a 6-character pairing code; receiver enters it to accept — no shared passphrase to coordinate
- Incoming transfers appear as toasts in the corner of every screen, with code input and Accept/Reject per transfer
- Recently-contacted devices remembered across restarts; appear in the peer picker alongside live-discovered devices
- Note payload is encrypted with a key derived from the pairing code (Argon2id + ChaCha20-Poly1305) before it leaves the sender

**Mobile**

- Collapsible sidebar with hamburger toggle
- Touch drag for Kanban
- Version shown at the bottom of the sidebar

---

## How transfer works

**Sending:** Open a note and tap **···** → **Send note**, or use multi-select on the notes list and tap **Send selected**. Pick a device from the peer list. The app generates a pairing code — tell the recipient the code.

**Receiving:** An incoming transfer appears as a toast notification. Enter the pairing code from the sender and tap **Accept**. The note is decrypted, re-encrypted with the local device key, and added to your notes list. Wrong code leaves the transfer pending so you can retry.

Peers are discovered automatically via mDNS and UDP broadcast beacon. The beacon covers networks where router multicast filtering blocks mDNS (e.g., WiFi + Ethernet on the same segment).

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

To run desktop and Android simultaneously on the same machine:

```bash
set TAURI_DEV_HOST=<your_LAN_IP>   # e.g. 172.16.0.101
npm run tauri android dev           # deploys to phone, starts Vite on 0.0.0.0:1420
./src-tauri/target/debug/panote.exe # run desktop binary directly
```

---

## Building

```bash
npm run tauri build
```

For Android:

```bash
npm run tauri android build           # release (unsigned)
npm run tauri android build -- --debug # debug (auto-signed, installs directly)
```

The debug APK is at `src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk`. Install with:

```bash
adb install src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
```

Desktop installers are output to `src-tauri/target/release/bundle/`.

---

## Project structure

```
panote/
├── src/                        # Svelte frontend
│   ├── routes/
│   │   ├── +layout.svelte      # Root layout; incoming transfer polling and toasts
│   │   ├── +page.svelte        # Notes list, search, multi-select transfer
│   │   └── note/[id]/          # Note editor with ··· send menu
│   └── lib/
│       ├── tauri.ts            # Tauri command bindings
│       ├── kanban.ts           # Kanban drag-and-drop logic
│       ├── stores/             # Svelte stores (notes)
│       └── components/         # Note type editors + TransferModal + IncomingTransferToast
└── src-tauri/                  # Rust backend
    └── src/
        ├── crypto/             # Encryption primitives, TLS, TOFU
        ├── db/                 # SQLite migrations (0001–0006) and queries
        ├── notes/              # Note CRUD commands
        ├── transfer/           # LAN (mDNS + beacon + TLS) and BLE transport
        └── state.rs            # Shared app state
```

---

## Storage

Notes are encrypted at rest with a 32-byte key generated on first launch and stored in the local SQLite database. The database lives in the OS app data directory and is never synced anywhere.

Copying the database to a different device will not work; the key does not travel with the file.

Transfer history (device names, last-transfer timestamps) is stored in the `known_peers` table.

---

## Security notes

- Transport uses TLS 1.3 with self-signed certificates and TOFU fingerprint pinning. Fingerprints are persisted across restarts. A changed fingerprint on reconnect is rejected.
- Note payloads are additionally encrypted with a key derived from the pairing code (Argon2id + ChaCha20-Poly1305) before transmission. A wrong code produces a decryption error; the transfer remains pending and the user can retry.
- Pairing codes are 6 characters from an unambiguous 32-character alphanumeric alphabet (≈30 bits). Each guess requires a full Argon2id KDF round on the receiver, making online brute force infeasible within any realistic transfer window.
- Peer display names and IDs received over the network are capped at 128 characters before storage.
- Markdown preview output is sanitized with DOMPurify before rendering.
- BLE transport is stubbed and not yet functional. The btleplug peripheral role is unsupported on Windows, and the feature is deferred to a future release.

---

## License

MIT
