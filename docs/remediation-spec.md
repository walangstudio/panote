# Remediation - 2026-07-28 review

Tracking doc for the 24 findings from the seven-persona review. Status here is the
source of truth; update the row when a fix lands.

Status: `todo` / `wip` / `done` / `deferred`

**Where this stands: all 24 done.**

Last green run: **241 frontend tests, 189 cargo tests**, svelte-check at the 8-error
baseline (all pre-existing, in TableEditor/ChecklistEditor/KanbanEditor and the note
route), release check and production build clean. Nothing committed; HEAD is still
`6004240`.

P1 was measured before changing, on the real database: all note bodies
(`content_ct`, 3 notes) came to **592 bytes**; a single `bg_image` was **84,987**.
The list query had dropped the small column and kept one 143x larger, and
`refreshNotes` re-shipped it on every save, pin, delete and incoming transfer.
Backgrounds now load once via `note_bg_images` and are cached.

## How the findings were produced

Seven persona agents reviewed the app read-only (first-run, power-user, security, mobile,
accessibility, data-integrity, writer). Four delivered; the other three scopes were covered
directly. Every severe claim was verified against source before being accepted.

Rule for this work: **each fix ships with a test watched failing first.** Several of these
findings existed behind a green typecheck and a green build.

---

## P0 - destroys data

| ID | Finding | Evidence | Status |
|----|---------|----------|--------|
| D1 | Tables and images in existing notes flattened on open+save. markdown-it parses them, the schema had no node, ProseMirror kept the text and dropped the structure, `onUpdate` wrote it back. | `editorExtensions.ts`, `markdownTable.ts` | done |
| D2 | Closing the window discards unsaved work. No autosave, no Ctrl+S, no close guard. | `note/[id]/+page.svelte` | done |
| D3 | Stray Enter permanently deletes a note - window-level Enter handler ignored focus, and there is no trash table. | `ConfirmModal.svelte` | done |
| D4 | Other markdown constructs silently rewritten: YAML front matter (`---` hits the `hr` rule), reference-link definitions, raw HTML blocks, setext headings, 4-space code, consecutive blank lines. | markdown-it default preset vs schema | done |

D2 was solved with the draft model rather than a close guard: edits autosave to an
encrypted `note_drafts` row and the committed note changes only on explicit save, so
losing the window is no longer destructive. Protected notes refuse drafts (a draft would
sit outside the password layer), and brand-new unsaved notes have no id to key on, so
that one case still relies on the dirty prompt.

## P1 - security

| ID | Finding | Evidence | Status |
|----|---------|----------|--------|
| S1 | `preview_text` stored unencrypted - 150 chars of note body in plain SQLite (and the `-wal`), defeating at-rest encryption for every unprotected note. | `0009_note_extras.sql:5`, `queries.rs` | done |
| S2 | Clipboard auto-clear cancelled on unmount - navigating away within 30s leaves the secret on the clipboard, contradicting its own tooltip. | `TableEditor.svelte` | done |
| S3 | Masked reveal/copy keyboard-inoperable - nested buttons swallowed by the row's `onkeydown`. Credentials were mouse-only. | `TableEditor.svelte` | done |
| S4 | `validate_bg_image` never runs on the import path - bypasses the 3MiB cap and MIME allowlist. | `export.rs` `insert_as_blob` | done |
| S5 | No rate limit on note-password guessing, though the pattern exists for LAN pairing and recovery codes. | `notes/commands.rs` unlock/protect impls | done |
| S6 | Password `String`s not zeroized at the IPC boundary, only once cached. | `notes/commands.rs` command signatures | done |
| S7 | `extract_preview` does not strip HTML, so highlight markup leaks into the list as tag soup. | `commands.rs` | done |

S1 note: the review found two write paths (`note_create`, `note_update`). A **third**
existed in `unprotect_impl`, which regenerates the preview when protection is removed -
fixing only the reported two would have left plaintext being written on that path.
Legacy plaintext rows decode-fail and are returned as-is, then re-encrypt on next save.

XSS surface was audited and is sound: zero `{@html}` sinks (the last one was dead code,
now deleted, along with 6 orphaned packages), CSP has `script-src 'self'` with no
`unsafe-inline`/`eval`, and the ProseMirror schema acts as an allowlist so unknown
elements cannot be constructed. Two independent URL guards proved load-bearing and are
documented at the config site: markdown-it's `validateLink` covers markdown link syntax,
TipTap's `isAllowedUri` covers raw HTML anchors. Neither covers the other - verified by
weakening each in turn. 16 hostile-payload tests.

## P2 - broken behaviour

| ID | Finding | Evidence | Status |
|----|---------|----------|--------|
| B1 | Delete in the editor's overflow menu did nothing - handler only closed the menu. | `note/[id]/+page.svelte` | done |
| B2 | 500-note cap: `noteList()` passes no limit, so note 501 ceases to exist app-wide. Pinned notes vanish from Pinned; Title sort silently sorts only the newest 500. | `tauri.ts`, `commands.rs` | done |
| B3 | Batch protect silently skips already-protected notes and reports success. | `commands.rs` | done |
| B4 | Batch unprotect throws after partial success, so `refreshNotes()` never ran and the list showed stale locks. | `NoteListPane.svelte` | done |
| B5 | Card ink never recomputed when a background image changed; `imgInk` never pruned. | `NoteListPane.svelte` | done |
| B6 | Search could not match `preview_text` despite it being printed on the card; protected notes excluded with no indication. | `NoteListPane.svelte` | done |
| B7 | Touch layout loses search, selection and scroll on every note open - below 900px the list is route-mounted. | `+page.svelte`, `stores/layout.ts` | done |

B6 note: excluding protected notes from search is deliberate and unchanged. Only the
silence was the bug; the list now shows "N locked notes hidden".

## P3 - accessibility (all done)

| ID | Finding | Status |
|----|---------|--------|
| A1 | Drawer tabbable while closed - 5 invisible tab stops on every page. Now `inert`. | done |
| A2 | No modal trapped or restored focus. Shared `trapFocus` action wired into all four modals. | done |
| A3 | `aria-pressed` never set; icon-only buttons announced the raw ligature ("format_bold") because the icon span's text beats `title`. | done |
| A4 | Masked cells announced eight bullet characters rather than "hidden". | done |
| A5 | `--muted` failed WCAG AA in both themes. Now 6.84:1 / 6.05:1 light, 5.73:1 / 5.18:1 dark. | done |
| A6 | No live regions anywhere. | done |
| A7 | No Escape in TransferModal/TableImportModal. | done |
| A8 | 32px tap targets, under the 44px touch minimum. | done |

A3 caveat: `aria-pressed` on **Bold** does not refresh on a collapsed cursor, because
toggling a mark with no selection only sets a stored mark and never fires TipTap's
`docChanged`. Pre-existing; the old CSS class had the same limitation.

## P4 - performance

| ID | Finding | Status |
|----|---------|--------|
| P1 | `bg_image` rides in every list row (up to ~4MiB base64), re-shipped on every save/pin/delete. The list query dropped `content_ct` and kept this. **Measure before changing.** | done |
| P2 | `formatRelative` built an `Intl.DateTimeFormat` per card. Hoisted. | done |
| P3 | Search query lowercased inside the filter callback, re-running per note per keystroke. | done |
| P4 | Per-card canvas luminance decoded full-resolution images and spread the whole `imgInk` map (O(k^2) allocation, k full list re-renders). Now keyed on image URL, batched, pruned. | done |
| P5 | Gamekit log grew unbounded in localStorage; `countWords` counted raw markup. Capped at 300 with snapshot folding. | done (fix lives on `experiment/gamekit`; the feature is not on this branch) |
| P6 | Descending sort had no stable secondary key, so flipping direction scrambled within-group order. | done |

---

## Accepted, not fixed

- Resolved: protected note **titles** were decrypted with the device key and visible in
  the list without the password. They are now sealed under the note password with the body
  and show as "Locked note" until unlocked; older notes migrate on their next unlock.
- On **Android** the device key lives in the app-private DB, not the Keystore.
- **Line length is uncapped** in the editor. The writer persona rates this a top-3
  problem (250+ chars maximised); the owner's call is that macOS Notes does not cap
  either. Standing.
- ~~**No trash/undo.** Delete is permanent, and the dialog says so.~~ Resolved: delete
  moves a note to Trash (migration 0016), restorable for 30 days.

## Process notes

Three of seven review agents and one of three fix agents went silent without reporting,
so roughly 40% of the work needed verifying or completing by hand. The reports that were
most useful were the ones that admitted uncertainty: one flagged a test of its own that
had never failed (contrast - since falsified by hand), another flagged that a computed
style assertion would have been vacuous under `css: false` and used a source-level guard
instead. Both were right to say so.
