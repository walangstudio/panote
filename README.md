<div align="center">

<img src="static/icon.png" width="96" alt="Panote" />

# Panote

[![Version](https://img.shields.io/badge/version-0.6.0-blue?style=flat-square)](src-tauri/tauri.conf.json)
[![Rust](https://img.shields.io/badge/Rust-1.78%2B-orange?style=flat-square&logo=rust&logoColor=white)](https://rust-lang.org)
[![Svelte](https://img.shields.io/badge/Svelte-5-ff3e00?style=flat-square&logo=svelte&logoColor=white)](https://svelte.dev)
[![License](https://img.shields.io/badge/License-AGPL--3.0-22c55e?style=flat-square)](LICENSE)

</div>

---

A local-first note-taking app for desktop and Android, built with Tauri 2, Svelte 5, and Rust. Notes are stored encrypted on-device. Transferring a note to another device uses a one-time pairing code; nothing goes through a server.

---

## Download

Installers are on the [releases page](https://github.com/walangstudio/panote/releases/latest):

| Platform | File |
|---|---|
| Windows | `.msi` or `.exe` (setup) |
| macOS (Apple silicon and Intel) | `.dmg` |
| Linux | `.AppImage`, `.deb` or `.rpm` |
| Android 7+ | `.apk` |

Desktop builds are not code-signed. Windows SmartScreen asks you to confirm ("More info", then "Run anyway"); on macOS, open the app once, then allow it under System Settings > Privacy & Security > Open Anyway.

---

## Features

**Note types**

- **Document** - WYSIWYG editor (TipTap): headings, lists, task lists, code blocks with syntax highlighting, links, quotes, text colour and highlight
- **Checklist** - nested check items with keyboard navigation
- **Kanban** - columns and cards, drag to reorder via handle; works on desktop (mouse) and Android (touch)
- **Table** - typed columns; `masked` columns hide values with per-cell reveal and copy (clipboard clears after 30 s). Imports `.env`, INI, browser password CSV, and JSON key/value

**Organising**

- Nested folders, browsed like a file manager; create and move notes and folders from the list
- Tags, pinning, and per-note background colour or image
- Sort by date edited, date created, title, kind, or Custom: drag notes and folders into your own order by the grip (mouse or touch), or move them with the arrow keys
- Search by title or tag
- Multi-select to protect, unprotect, or send several notes at once
- Trash: deleted notes can be restored for 30 days, then are purged
- Desktop split view above 900px window width
- Right-click a note or folder (or use its ··· button) for its menu, including Send… and, on folders, Receive into folder
- Copy, cut and paste notes and folders, file-manager style: from the row menu or Ctrl/Cmd+C, X, V. Paste lands in the folder you are viewing (or "Paste into" a folder); a copied protected note keeps its password
- Shortcuts: Ctrl/Cmd+N new note, Ctrl/Cmd+Shift+N new folder, Ctrl/Cmd+F search (find in note inside the editor), Ctrl/Cmd+C/X/V copy, cut and paste, Delete move to Trash, Ctrl/Cmd+S save, Esc close a menu or dialog
- Find in note (Ctrl/Cmd+F in the editor) for every note kind, with a match count and next/previous
- Optional autosave (Settings > Editing): the open note saves itself as you edit and before the window closes. Off by default; then leaving or closing with unsaved changes asks first

**Protection**

- Per-note passwords (Argon2id + ChaCha20-Poly1305) with recovery codes. The title is sealed with the body and listed as "Locked note" until unlocked
- Unlocked notes re-lock after 15 minutes of inactivity

**Transfer**

- LAN peer discovery via mDNS and UDP broadcast beacon (works across WiFi/Ethernet boundaries where mDNS multicast is filtered)
- Send from any note using the **···** menu, or from the notes list in multi-select mode
- Sender generates a 6-character pairing code; receiver enters it to accept. Pairing uses SPAKE2, so the code never crosses the wire
- Peers can also be paired by QR code, and recently-contacted devices are remembered
- Folders and protected notes survive a transfer
- **Send to a camera**: no network at all. The sender plays the notes as moving QR codes and the receiver reads them with its camera. Both sides type the same passphrase
- Send and Receive have tabs: **Camera** (default), **Network** (same Wi-Fi) and **Bluetooth** (coming soon). Received notes land in the folder you started the receive from

**Backup**

- Export and import from Settings (format v2; v1 backups are upgraded on import). Protected notes stay encrypted under their own password in the backup

---

## How transfer works

**Over the network:** Choose **Send…** on a note or folder (or **Send selected** in multi-select) and switch to the **Network** tab. Pick a device from the peer list. The app generates a pairing code; tell the recipient the code.

**Receiving over the network:** An incoming transfer appears as a toast notification; if a Receive dialog is open, it lands in that dialog's folder. Enter the pairing code from the sender and tap **Accept**. The note is decrypted, re-encrypted with the local device key, and added to your notes list. Wrong code leaves the transfer pending so you can retry.

Peers are discovered automatically via mDNS and UDP broadcast beacon. The beacon covers networks where router multicast filtering blocks mDNS (e.g., WiFi + Ethernet on the same segment).

**By camera:** Choose **Send…** on a note or folder; the dialog opens on the **Camera** tab. Type a passphrase of at least 10 characters and hold the screen up to the other device. There, choose **Receive** (beside +, in the + menu, or **Receive into folder** on a folder), point the camera at the codes, and type the same passphrase once they are received. Missed frames only slow it down: the stream is fountain-coded, so the receiver needs any set of frames that covers the payload, in any order.

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

Checks CI runs on every pull request to `main` (`.github/workflows/tests.yml`); run them before pushing:

```bash
npm run check                                  # svelte-check
npm test                                       # vitest
npm run test:e2e                               # Playwright
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
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

### Releasing

Bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, date its `CHANGELOG.md` section, then push a `vX.Y.Z` tag. The Release workflow builds every installer into a draft release whose notes are that CHANGELOG section; publish it once the files check out. The APK is signed only when the `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD` and `ANDROID_KEY_ALIAS` repository secrets are set; without them the release ships no APK.

---

## Project structure

```
panote/
├── src/                        # Svelte frontend
│   ├── routes/
│   │   ├── +layout.svelte      # Root layout; incoming transfer polling and toasts
│   │   ├── +page.svelte        # Notes list, search, multi-select transfer
│   │   ├── settings/           # Import/export
│   │   └── note/[id]/          # Note editor with ··· send menu
│   └── lib/
│       ├── tauri.ts            # Tauri command bindings
│       ├── kanban.ts           # Kanban drag-and-drop logic
│       ├── tableParsers.ts     # Table importers (.env, INI, CSV, JSON)
│       ├── stores/             # Svelte stores (notes)
│       └── components/         # Note type editors + TransferModal + IncomingTransferToast
└── src-tauri/                  # Rust backend
    └── src/
        ├── crypto/             # Encryption primitives, TLS, TOFU
        ├── db/                 # SQLite migrations and queries
        ├── folders/            # Folder commands
        ├── notes/              # Note CRUD commands
        ├── transfer/           # LAN (mDNS + beacon + TLS) and BLE transport
        └── state.rs            # Shared app state
```

---

## Storage

Notes are encrypted at rest with a 32-byte device key generated on first launch. On desktop the key lives in the OS keychain; on Android it is kept in the app-private database. The database lives in the OS app data directory and is never synced anywhere.

Copying the database to a different device will not work; the key does not travel with the file.

Transfer history (device names, last-transfer timestamps) is stored in the `known_peers` table.

---

## Security notes

- Transport uses TLS 1.3 with self-signed certificates and TOFU fingerprint pinning. Fingerprints are persisted across restarts. A changed fingerprint on reconnect is rejected.
- Inside TLS, both sides run SPAKE2 on the pairing code with HMAC key confirmation, then encrypt the payload under the derived key. The code never crosses the wire, a wrong code aborts the transfer, and each guess needs a live round.
- Pairing codes are 6 characters from an unambiguous 32-character alphanumeric alphabet (≈30 bits). A peer is locked out after 5 wrong codes.
- Peer display names and IDs received over the network are capped at 128 characters before storage.
- BLE transport is stubbed and not yet functional. The btleplug peripheral role is unsupported on Windows, and the feature is deferred to a future release.
- Camera transfer: anyone who can film the sender's screen captures every frame, so the payload is sealed before it is drawn (Argon2id over a fresh salt, ChaCha20-Poly1305). The passphrase is typed on both devices and never shown on screen.

---

## Third-party code

Camera transfer uses [Decimen Optical Transfer](https://github.com/bashalarmistalt/decimen-optical-transfer) v0.5.3 (AGPL-3.0-or-later, Copyright (c) 2026 Evan Crawley): its wire protocol, fountain code and decoder are vendored unmodified in `src/lib/vendor/decimen/`, and its send and receive loops are adapted in `src/lib/optical/`. The decoder is decimen-codec, a WebAssembly build of [zxing-cpp](https://github.com/zxing-cpp/zxing-cpp) (Apache-2.0); its source is at [bashalarmistalt/decimen-codec](https://github.com/bashalarmistalt/decimen-codec). Notices are in `src/lib/vendor/decimen/NOTICE` and `src/lib/vendor/decimen/vendor/decimen-codec/`.

---

## License

Copyright (C) 2026 walangstudio

[GNU Affero General Public License v3.0 or later](LICENSE). Releases up to and including 0.5.0 were MIT-licensed and remain available under those terms.
