# Panote — Design Spec

Self-contained design handoff for Panote. Everything below is extracted from the shipping UI (Svelte 5 + Tauri 2, desktop + Android). No repo access required to consume this doc.

## 1. Product

Offline-first, encrypted note-taking app for desktop and Android, with LAN-to-LAN note transfer (no cloud). Four note kinds: **Document** (rich text / markdown / code), **Checklist**, **Kanban**, **Table**. Notes can be per-note password protected, pinned, tagged, and given a background color or image.

Design intent: friendly, soft, "candy" aesthetic. Glassmorphism over a pink-to-violet gradient. Pill-shaped controls, generous radii, light motion. Reads as a consumer productivity app, not an enterprise tool.

## 2. Themes

Two themes, switched via `data-theme` on `<html>`. The preference is persisted to `localStorage["panote-theme"]` (and the device DB) as one of `candy-light`, `candy-dark`, or `system`. Default `system`: it resolves to `candy-dark` when `prefers-color-scheme: dark` matches and follows the OS live. An inline script in `app.html` applies the resolved theme before hydration.

- `candy-light`
- `candy-dark`
- `system` (default; resolves to one of the above)

All color, radius, and shadow values are CSS custom properties scoped to the theme selector. Components never hardcode color except two intentional cases: cards with a user-set light background pin text to dark literals (`#2e1a28` / `#604868`).

## 3. Color tokens

| Token | candy-light | candy-dark | Role |
|---|---|---|---|
| `--bg` | `#fef7ff` | `#1a1625` | Base background |
| `--bg-gradient` | `linear-gradient(135deg,#fef7ff 0%,#ffd6ee 50%,#eedcff 100%)` | `linear-gradient(135deg,#1a1625 0%,#2a1530 50%,#1e1a30 100%)` | Page gradient (layered over `--bg`) |
| `--surface` | `#ffffff` | `#2b2538` | Solid card surface |
| `--surface-glass` | `rgba(255,255,255,0.7)` | `rgba(43,37,56,0.8)` | Glass surfaces (blurred) |
| `--surface-container` | `#f8eef8` | `#322c42` | Inset fields, search, chips |
| `--surface-high` | `#f2e8f2` | `#3d3650` | Raised inner surface |
| `--hover` | `#fdf0f8` | `#3a3350` | Hover fill |
| `--text` | `#2e1a28` | `#e6e0ec` | Primary text |
| `--text-secondary` | `#604868` | `#b8a8c8` | Secondary text |
| `--muted` | `#907898` | `#8a7a98` | Muted / placeholder / meta |
| `--border` | `#e8d8ec` | `#4a4260` | Hairline borders |
| `--input-bg` | `#ffffff` | `#241e32` | Input background |
| `--accent` | `#e040a0` | `#f06cb8` | Primary (candy pink/magenta) |
| `--accent-hover` | `#c4358a` | `#e040a0` | Primary hover |
| `--accent-muted` | `rgba(224,64,160,0.08)` | `rgba(240,108,184,0.12)` | Tint fills, focus ring |
| `--accent-surface` | `#ffd6ee` | `#3d1a30` | Accent badge background |
| `--on-accent` | `#ffffff` | `#ffffff` | Text/icon on accent |
| `--secondary` | `#7c52aa` | `#b08cda` | Secondary accent (violet) |
| `--secondary-surface` | `#eedcff` | `#2e2040` | Secondary badge bg |
| `--tertiary` | `#0096cc` | `#40b8e0` | Tertiary accent (cyan) |
| `--tertiary-surface` | `#c8eaff` | `#0a2838` | Tertiary badge bg |
| `--error` | `#e53e3e` | `#f06060` | Destructive |
| `--error-surface` | `#ffe8e8` | `#3d1a1a` | Destructive tint |
| `--success` | `#27ae60` | `#40c870` | Success |
| `--shadow-color` | `rgba(224,64,160,0.08)` | `rgba(0,0,0,0.2)` | Resting shadow (pink-tinted in light) |
| `--shadow-color-hover` | `rgba(224,64,160,0.15)` | `rgba(0,0,0,0.3)` | Elevated shadow |

Accent system is three-channel: **accent** (pink) for primary/documents, **secondary** (violet) for markdown/table, **tertiary** (cyan) for checklist/kanban. Each has a matching `-surface` for badge backgrounds.

## 4. Typography

Body font: **DM Sans** (Google Fonts), weights 400 / 500 / 700 / 900. Fallback stack: `-apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif`.

Icon font: **Material Symbols Outlined** (variable: `FILL 0..1`, `wght 100..700`, `opsz 24`). Active/selected states toggle `FILL` 0→1 rather than swapping glyphs. Default settings: `FILL 0, wght 400, GRAD 0, opsz 24`.

Type scale (observed, rem):

| Use | Size | Weight |
|---|---|---|
| Logo wordmark | 1.5 | 900 |
| Editor title input | 1.2 | 900 |
| Modal heading (h2) | 1.1 | 700 |
| Note card title | 0.95 | 700 |
| Primary button / body | 0.9–0.95 | 500–700 |
| Search / inputs | 0.9 | 400 |
| Nav item, secondary labels | 0.85–0.9 | 500 |
| Preview text, date, meta | 0.78 | 500 |
| Tag chip (list) | 0.68 | 600 |

Weight 900 is the brand voice: used only for the wordmark and the editor title. Heavy weight + tight tracking (`letter-spacing: -0.02em` on logo).

## 5. Spacing, radius, elevation

**Radius scale:**
- `--radius-sm` `0.5rem` — menu items, small fields
- `--radius` `1rem` — cards, toolbar, modals' inner, dropdowns
- `--radius-lg` `1.5rem` — modals
- `--radius-full` `9999px` — buttons, pills, chips, nav items, FAB, search
- Kind badge uses a literal `12px`; toggle pill `11px`.

Pill-shaped (`--radius-full`) is the default for anything interactive and text-bearing. Cards and containers use `--radius` / `--radius-lg`.

**Shadows (elevation ladder):**
- Toolbar: `0 2px 12px var(--shadow-color)`
- Card rest: `0 4px 16px var(--shadow-color)`
- Card hover: `0 8px 24px var(--shadow-color-hover)` + `translateY(-2px)`
- Dropdown / popover: `0 8px 24px var(--shadow-color-hover)`
- FAB: `0 6px 20px var(--shadow-color-hover)`
- Drawer: `8px 0 32px var(--shadow-color-hover)`
- Modal: `0 16px 48px var(--shadow-color-hover)`

In light theme shadows are pink-tinted (not neutral gray), reinforcing the candy palette.

**Glassmorphism:** `background: var(--surface-glass)` + `backdrop-filter: blur(12px–16px)` (always paired with `-webkit-` prefix). Applied to toolbar, sort/context dropdowns, modals, sidebar drawer, bottom action bar, FAB speed-dial options, incoming-transfer toasts. Solid `--surface` is used for resting note cards only.

**Background:** every screen sits on `background: var(--bg-gradient), var(--bg)` set on `body`. Content scrolls over a fixed gradient.

## 6. Motion

- Hover/color transitions: `0.15s ease` (sometimes `all 0.15s`).
- Card lift + shadow: `0.2s ease`, `transform: translateY(-2px)`.
- Drawer slide: `transform 0.25s ease` (`translateX(-100%)` → `0`).
- Press feedback: `scale(0.97)` active on primary buttons; `scale(1.02–1.08)` on hover.
- FAB: icon `rotate(45deg)` when open (the `add` glyph becomes a close `×`).
- FAB speed-dial: staggered entrance, `fab-pop` keyframe (`opacity 0→1`, `translateY(8px) scale(0.9)→1`), `40ms * index` delay.
- Hover nudges: list FAB options shift `translateX(-4px)` on hover.

Keep motion light and spring-free. No long durations, no bounce.

## 7. Iconography

Material Symbols Outlined throughout. Key mappings:

| Concept | Icon |
|---|---|
| Document | `edit_note` (markdown), `description` (plain), `code` (code) |
| Checklist | `checklist` |
| Kanban | `view_kanban` |
| Table | `table_chart` |
| Menu / drawer | `menu`, `close` |
| Search | `search` |
| Sort | `swap_vert`, `arrow_upward`/`arrow_downward` |
| Overflow | `more_vert` |
| Pin | `push_pin` |
| Lock states | `lock`, `lock_open`, `password` |
| Create | `add` |
| Receive | `download` |
| Theme | `dark_mode` / `light_mode` |
| Empty state | `note_add` |

Badge color follows the note kind's accent channel (see §3).

## 8. Components

### Buttons
- **Primary**: `--accent` fill, `--on-accent` text, `--radius-full`, weight 600–700, shadow `0 2px 8px`, hover `scale(1.03)`, active `scale(0.97)`. (`.btn-send`, `.btn-confirm`, `.new-note-btn`, FAB.)
- **Ghost icon**: 40×40, transparent, `1px --border`, `--text-secondary`; hover → `--accent` text + border + `--accent-muted` fill. (`.btn-ghost`.)
- **Cancel/secondary**: transparent, `1px --border`, `--muted` text; hover → accent border + text. (`.btn-cancel`, `.select-btn`.)
- **Destructive**: same as primary but `--error` fill (`.btn-confirm.destructive`); destructive menu items use `--error` text.
- **Bare icon button**: no border/bg, `--text-secondary`; hover → `--accent` + `--accent-muted` pill fill. (`.menu-btn`, `.sort-btn`, `.card-menu`, `.header-menu-btn`.)
- **Round back/close button**: 32px, `--accent-muted` fill, hover → solid `--accent` + `--on-accent`. (`.close-btn`, `.back`, `.lock-btn`.)

### FAB + speed-dial
56×56 circle, accent fill, fixed bottom-right (`right 1.5rem`, `bottom calc(1.5rem + safe-area)`). Tapping fans out a vertical stack of labeled pill options (one per note kind), with a dimming backdrop (`rgba(0,0,0,0.25)`). Icon rotates 45° to act as the close affordance.

### Search field
Full-width pill, `--surface-container` bg, leading `search` icon, no border. Focus ring `box-shadow: 0 0 0 2px var(--accent-muted)`. Placeholder in `--muted`.

### Toggle pill (switch)
40×22, track `11px` radius. Off: `--surface-container` track + `--muted` knob. On: `--accent` track + `--on-accent` knob, knob slides `left 2px → 20px`. (Used for "Receiving".)

### Note card
Solid `--surface`, `--radius`, `0 4px 16px` shadow, hover lifts `-2px` with stronger shadow + faint accent border. Layout: kind badge (40×40, `12px` radius, channel-colored surface+icon) · info column (bold title, optional lock icon, 2-line clamped preview, tag chips) · trailing date. Pinned cards show an 18px accent dot with a filled `push_pin`, top-right of the badge. A trailing `more_vert` opens a glass popover (Pin, View, Edit, Set/Change/Remove password, Delete). Selection mode swaps the badge row for a checkbox and tints the card `--accent-muted` when checked. Cards accept a user background color or image (image gets a 55% white scrim).

### Tag chip
`--accent-muted` fill, `--accent` text, `--radius-full`, weight 600, tiny (0.68rem in list). On light user-bg cards, chips fall back to `rgba(0,0,0,0.08)` + dark text.

### Sidebar drawer
Off-canvas left drawer, 280px, glass surface, slides in over a blurred dark backdrop (`rgba(0,0,0,0.45)` + `blur(4px)`). Contents top→bottom: wordmark + close, full-width "New Note" primary button, nav items (Notes, Settings), then a bottom group with the Receiving toggle row and the theme picker (Light / Dark / System). Active nav item is a solid accent pill with `--on-accent` text; inactive items are `--text-secondary` and fill `--hover` on hover. Active icon uses `FILL 1`.

### Toolbar (notes list)
Glass bar, `--radius`, contains: hamburger (`menu`) · search · sort button · Select toggle. Sticky feel via blur; `z-index: 10`.

### Dropdown / popover menu
Glass surface, `--radius`, 1px border, `0 8px 24px` shadow, small padding. Items are `--radius-sm`, `--text-secondary`, hover → `--hover` + `--text`. Active item → `--accent` + weight 600. Destructive items → `--error`. Always rendered with a transparent full-screen backdrop for click-away.

### Modal
Centered, `width: min(400px, 92vw)`, glass surface, `--radius-lg`, `1.5rem 1.75rem` padding, `0 16px 48px` shadow, over a `rgba(0,0,0,0.45)` + `blur(4px)` backdrop. h2 title (1.1rem/700), secondary message, right-aligned actions (cancel + confirm). Escape cancels, Enter confirms. ARIA: `role="dialog"`, `aria-modal`, `aria-labelledby`.

### Bottom action bar (batch select)
Fixed bottom, glass, top border, respects bottom safe-area. Left: "{n} selected". Right: ghost icon buttons (lock / unlock) + primary "Send selected".

### Empty state
Centered column, 48px `note_add` icon at 40% opacity, muted helper text ("No notes yet. Tap + to create one.").

## 9. Screens

1. **Notes list** (`/`) — toolbar, vertical card list (max-width 800px, centered), FAB bottom-right, optional batch action bar. Pinned notes sort first; sort field/dir persisted.
2. **Note editor** (`/note/[id]`, `new` for create) — glass header with round back button, weight-900 title input, kind pill, overflow menu (transfer, background, password, delete), per-kind editor body (Markdown / Checklist / Kanban / Table), tag input row, background color/image picker. Locked notes show a lock gate before content. Tracks dirty state and warns on navigate-away.
3. **Settings** (`/settings`) — device identity, LAN transfer/receive, theme, security (per-note password defaults, keychain), about.
4. **Sidebar drawer** — global nav overlay (see §8).
5. **Modals** — New Note (kind picker), Password (set/change/unlock/remove), Transfer (with QR show), QR scan, Confirm (destructive). Incoming-transfer **toasts** stack as glass cards.

## 10. Responsive & platform

- Centered content, `max-width: 800px`.
- Breakpoint `640px`: tighter page padding, smaller search/select, title input drops to 1rem.
- Android/iOS safe areas: `body` pads `env(safe-area-inset-top/bottom)`; FAB, action bar, and list bottom add `env(safe-area-inset-bottom)`.
- Touch-first sizing: 40–56px tap targets on primary controls.

**Z-index ladder:** toolbar 10 · dropdown/popover 19–20 · action bar 60 · FAB backdrop 70 · FAB 80 · drawer backdrop 80 · drawer 81 · modal backdrop 100 · modal 101.

## 11. Accessibility

Present today: `aria-label` on icon-only buttons, `role="dialog"`/`aria-modal`/`aria-labelledby` on modals, `role="presentation"` on click-away backdrops, Escape/Enter handling on Confirm modal, focus ring on search.

Gaps to address in any redesign: verify contrast of `--muted` text on glass surfaces (especially `--muted` on `--surface-container`), focus-visible styles on all interactive elements (only search has a visible ring), keyboard traversal of dropdowns/popovers, and reduced-motion handling (`prefers-reduced-motion` not yet honored for the FAB/drawer animations).

## 12. Assets

Fonts loaded from Google Fonts CDN in `app.html`:
```
DM Sans: wght 400;500;700;900
Material Symbols Outlined: wght,FILL@100..700,0..1
```
App icons: `static/icon.png`, `static/favicon.png`.

## 13. Handoff checklist

- [ ] Honor both themes; never hardcode color outside the two documented light-bg exceptions.
- [ ] Reuse the three accent channels (pink/violet/cyan) for any new note-kind or status color.
- [ ] New surfaces: glass (`--surface-glass` + blur) for floating/overlay, solid `--surface` for resting content.
- [ ] New interactive text controls default to pill (`--radius-full`).
- [ ] New shadows pull from the elevation ladder (§5), pink-tinted in light theme.
- [ ] Active states toggle Material Symbols `FILL` 0→1, not glyph swaps.
- [ ] Respect safe-area insets on any fixed/bottom element.
- [ ] Provide focus-visible + reduced-motion for anything new (closes existing gaps).
