# gamekit in panote

Gamification (writer levels, daily streak, badges) via [gamekit](../../../../../react/gamekit) —
a framework-agnostic, zero-dependency event-sourced engine. This proves gamekit runs inside a
**Tauri (SvelteKit) webview** with no platform-specific code.

## Files
- `definitions.ts` — the rules: `notes`/`words` scores, a daily writing streak, a writer tier,
  and 7 badges (incl. `night_owl` via gamekit 0.2's `count.todBetween`).
- `store.ts` — the engine wired to Svelte: a lazy, SSR-safe singleton; the event log is persisted
  to `localStorage` and replayed on startup (real persistence, no Rust). Exposes the `gameStats`
  Svelte store, `initGamekit()`, and `recordNoteSaved()`.
- `definitions.test.ts` — vitest over the rules (engine + in-memory stores).

## Wiring
- `routes/note/[id]/+page.svelte` `save()` calls `recordNoteSaved({ isNew, kind, content })`
  after a successful create/update (failures are swallowed — never block a save).
- `routes/settings/+page.svelte` shows a "Progress" panel bound to `gameStats`.

## Why this is the webview-only integration
gamekit's core is pure ESM with no platform globals, so it bundles into the SvelteKit frontend
exactly like any web dep. Persistence is the only platform-specific piece, and here it's the
webview's `localStorage`. For multi-device sync or hardened/encrypted storage, swap the in-memory
stores in `store.ts` for a **Rust/sqlx adapter** behind Tauri `invoke` (implement the four store
ports against the existing SQLite DB) — `definitions.ts` and the public API stay identical.

## Verify
```
npm test               # vitest — includes the gamekit rules
npm run build          # vite build — confirms it bundles into the Tauri frontend
```
`tauri dev` runs it in the real desktop webview (the Rust side is unchanged by this feature).
