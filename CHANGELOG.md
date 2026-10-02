# Changelog

## [Unreleased]

### Added

- A **System** theme that follows the OS light/dark setting and switches live when it
  changes. It is the default on a fresh install; an existing Light or Dark choice is kept.
  Pick it from the sidebar or Settings > Appearance.
- **Trash.** Deleting a note moves it to Trash instead of destroying it. Trash is in the
  sidebar and lists each note with its deleted date, with Restore and Delete forever per
  note and Empty trash. A restored note returns to its folder, or to the root if that
  folder is gone. Notes trashed more than 30 days ago are purged at startup. Protected
  notes stay encrypted in Trash and keep their password when restored. Trashed notes are
  left out of the list, counts, search, folder counts, export and folder sends; a note
  re-sent from another device comes back out of Trash.

### Fixed

- **Custom sort is back, and its drag works.** Rows now follow the pointer while dragging
  (the note list painted its pre-drag order), a drop lands where it was let go (Custom took
  the Descending direction and showed every saved order upside down), and folders keep
  their arranged order (they were always listed by name). Drag the grip with a mouse or a
  finger; the rest of the row still scrolls. A focused grip moves its row with the up and
  down arrow keys, announced to screen readers. Pinned notes stay on top and are arranged
  among themselves. Custom has no direction, and grips hide while searching.

### Security

- **Protected note titles are sealed under the note password, like the body.** Titles used
  to sit under the device key alone, so anyone with the unlocked app, or the database plus
  the OS keychain, could read them. The list, search and sorting now see "Locked note" until
  the note is unlocked this session, and again after it re-locks. Removing the password puts
  the title back under the device key.
- **Existing protected notes migrate lazily.** Their title is sealed, and the device-key copy
  dropped, the next time each one is unlocked. Until then the old copy stays on disk, so
  unlock every protected note once to finish the migration.
- Export seals a protected note's title in its encrypted blob and writes "Locked note" as the
  entry title. Older backups (title on the entry) still import; older builds import a new
  backup with the title "Locked note". Transfer is unchanged on the wire: the title travels
  with the body and is re-sealed on a receiver that protects the note.

## [0.4.0] - 2026-09-25

### Breaking

- **Document notes are now edited in a WYSIWYG editor (TipTap/ProseMirror), replacing the
  markdown textarea and its Edit/Preview tabs.** The editor rewrites a note's markdown when
  it saves, so formatting that the old editor stored in its own syntax will not survive
  untouched. **Export your notes before updating, then import the backup afterwards** —
  import upgrades old backups automatically (see below).
- **Coloured text changed representation.** It used to be stored as KaTeX inline maths
  (`$\textcolor{#hex}{\text{...}}$`); it is now an inline span (`<span style="color:#hex">`).
- Export format is now **v2**. `notes_import` reads v1 and v2; v1 files are upgraded on
  import, converting the old colour syntax and unescaping text that was escaped to survive
  inside `\text{}`. v1 backups therefore keep working — but a v2 backup cannot be read by
  an older build, which refuses it with a "newer version of panote" error.

### Security

- **Backups no longer contain password-protected notes in the clear.** Export used to peel
  the password layer and write plaintext JSON. Protected notes are now re-encrypted under
  their own password (Argon2id over a fresh salt, ChaCha20-Poly1305, bound to the note id so
  a blob can't be moved between entries) and import asks for that password. Notes that can't
  be decrypted are reported, never silently dropped.
- **Unlocked notes now re-lock after 15 minutes of inactivity.** Cached passwords used to be
  held until the app exited. They are now timestamped, zeroized on drop, and swept from
  memory on any access — not merely ignored.
- **Transfer pairing runs SPAKE2 with key confirmation inside TLS.** The pairing code no
  longer crosses the wire, and a peer is locked out after 5 wrong codes.
- **The device key moved to the OS keychain on desktop.** Android keeps it in the
  app-private database.

### Added

- Nested folders, browsed like a file manager. Notes and folders are created and moved
  from the list, and folders are carried across a transfer.
- Per-note passwords with recovery codes, settable on many notes at once from
  multi-select.
- Table columns have a type. `masked` renders the value as fixed-width dots (the mask does
  not reveal length) with per-cell reveal and copy. Copying clears the clipboard after 30
  seconds, and only if the secret is still on it.
- Importers for `.env`, INI, browser password CSV, and JSON key/value objects. `.env` values
  and a recognised password column arrive masked automatically.
- Desktop split view: a persistent note list beside the note, macOS Notes style, above a
  900px window width. Narrower windows keep the existing touch layout unchanged.
- Rich editor: headings, bold/italic/strikethrough, bullet/numbered/task lists, inline code,
  syntax-highlighted code blocks, links, quotes, horizontal rules, emoji, text colour and
  highlight. Tables are deliberately excluded — use the dedicated Table note type, which
  round-trips reliably.

### Fixed

- Coloured text rendered as raw `\textcolor{...}` LaTeX in the preview. DOMPurify strips
  MathML `<semantics>`/`<annotation>` by default and, with `KEEP_CONTENT` on, spilled the
  annotation's TeX source into the output as text.
- Fonts are now self-hosted; the app made a Google Fonts request on every cold start and
  showed no icons at all when offline. Nothing in the app touches the network now.
- Dark theme rendered native controls with light-mode colours — `<option>` text came out
  black on a dark background in the import dialog. The themes now declare `color-scheme`.

### Performance

- The notes list query no longer reads note body ciphertext it immediately discards.
- Sorting no longer re-runs on every search keystroke, and uses a shared `Intl.Collator`.
- Dropped the unused `idx_notes_kind` index.

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
