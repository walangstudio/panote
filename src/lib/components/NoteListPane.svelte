<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { notes, totalNotes, bgImages, refreshNotes, sortPref, sortNotes, type SortField } from "$lib/stores/notes";
  import {
    noteDelete, notePin,
    noteProtect, noteUnprotect, noteChangePassword, notesProtect, notesUnprotect,
  } from "$lib/tauri";
  import type { NoteMetadata } from "$lib/tauri";
  import { get } from "svelte/store";
  import { sidebarOpen } from "$lib/stores/sidebar";
  import { listFilter, listSelecting, listSelected, listFolder } from "$lib/stores/listState";
  import ConfirmModal from "$lib/components/ConfirmModal.svelte";
  import PasswordModal from "$lib/components/PasswordModal.svelte";
  import NewNoteModal from "$lib/components/NewNoteModal.svelte";

  interface Props { desktop?: boolean; }
  let { desktop = false }: Props = $props();

  // Seeded from the stores so a round trip to a note and back does not wipe the
  // search or the selection, then written back as they change. On desktop the
  // pane never unmounts, so this is a no-op there.
  let filter = $state(get(listFilter));
  let selecting = $state(get(listSelecting));
  let selected = $state(get(listSelected));

  $effect(() => { listFilter.set(filter); });
  $effect(() => { listSelecting.set(selecting); });
  $effect(() => { listSelected.set(selected); });
  let transferNoteIds = $state<string[] | null>(null);
  let deleteTargetId = $state<string | null>(null);
  let sortOpen = $state(false);
  let menuNoteId = $state<string | null>(null);
  let fabOpen = $state(false);
  let showNewNote = $state(false);

  const activeId = $derived(page.params.id ?? "");

  type PwMode = "set" | "change" | "remove";
  let pwModal = $state<{ mode: PwMode; ids: string[]; isBatch: boolean } | null>(null);

  // Only batch set/remove override the modal's default heading; everything else
  // returns undefined so PasswordModal falls back to its own per-mode title.
  const pwHeading = $derived(
    pwModal?.isBatch && pwModal.mode !== "change"
      ? pwModal.mode === "set"
        ? `Protect ${pwModal.ids.length} notes`
        : `Remove protection from ${pwModal.ids.length} notes`
      : undefined,
  );

  async function handlePassword(v: { password: string; oldPassword?: string }) {
    if (!pwModal) return;
    const { mode, ids, isBatch } = pwModal;
    try {
      if (mode === "set") {
        if (isBatch) await notesProtect(ids, v.password);
        else await noteProtect(ids[0], v.password);
      } else if (mode === "change") {
        await noteChangePassword(ids[0], v.oldPassword ?? "", v.password);
      } else {
        if (isBatch) await notesUnprotect(ids, v.password);
        else await noteUnprotect(ids[0], v.password);
      }
    } finally {
      // A batch call reports failure only after it has already changed some
      // notes, so the list is stale whether or not it threw. Refresh either way;
      // the error keeps propagating, so the modal still shows what went wrong.
      await refreshNotes();
      if (isBatch) { selecting = false; selected = new Set(); }
    }
  }

  // Bottom-up speed-dial: last item sits nearest the FAB (prototype order).
  const fabKinds = [
    { id: "table", icon: "table_chart", label: "Table" },
    { id: "kanban", icon: "view_kanban", label: "Kanban" },
    { id: "checklist", icon: "checklist", label: "Checklist" },
    { id: "document", icon: "edit_note", label: "Document" },
  ] as const;

  const sortOptions: { field: SortField; label: string }[] = [
    { field: "updated", label: "Date edited" },
    { field: "created", label: "Date created" },
    { field: "title", label: "Title" },
    { field: "kind", label: "Kind" },
  ];

  // Backgrounds are fetched here and then only when something could have
  // changed them — not on every pin, delete or save.
  onMount(() => { refreshNotes({ withBackgrounds: true }); });

  // ponytail: defense in depth — backend already validates bg_image is a data:image/... URI.
  function safeBgImageUrl(bgImage: string | null | undefined): string | undefined {
    return bgImage && bgImage.startsWith("data:image/") ? `url(${bgImage})` : undefined;
  }

  // Sorting is O(n log n) and only depends on the notes + pref, so it must not
  // rerun on every keystroke — filtering the sorted result is the cheap part.
  const sorted = $derived(sortNotes($notes, $sortPref));
  // Normalised once per keystroke, not once per note per keystroke.
  const query = $derived(filter.trim().toLowerCase());
  // preview_text is printed on the card, so a phrase the user can read off the
  // screen has to be findable too.
  const matches = (n: NoteMetadata) =>
    n.title.toLowerCase().includes(query) ||
    n.tags.some(t => t.toLowerCase().includes(query)) ||
    (n.preview_text?.toLowerCase().includes(query) ?? false);
  // A folder narrows the list; a search then applies within it. Search does not
  // escape the folder you are looking at — a result you cannot see in context is
  // more confusing than a short list.
  const inFolder = $derived(
    $listFolder === null ? sorted : sorted.filter(n => n.folder_id === $listFolder),
  );
  const filtered = $derived(
    // No query → show everything, including secret notes. With one, secret notes
    // are never searchable — not by title, tags, preview, or anything.
    query ? inFolder.filter(n => !n.has_note_password && matches(n)) : inFolder,
  );
  // Holding locked notes back is deliberate, but silently returning nothing reads
  // as broken search, so the list owns up to what it withheld.
  const hiddenLocked = $derived(
    query ? inFolder.filter(n => n.has_note_password).length : 0,
  );

  /// How many notes the backend page cap is leaving out. Only meaningful with no
  /// search active — with a query the list is deliberately a subset.
  const notShown = $derived(query ? 0 : Math.max(0, $totalNotes - $notes.length));

  function doDelete(id: string) {
    deleteTargetId = id;
  }

  async function confirmDelete() {
    if (!deleteTargetId) return;
    const id = deleteTargetId;
    deleteTargetId = null;
    await noteDelete(id);
    await refreshNotes();
    // The deleted note may be the one open in the detail pane.
    if (desktop && activeId === id) goto("/");
  }

  async function togglePin(id: string, currentPinned: boolean) {
    await notePin(id, !currentPinned);
    await refreshNotes();
  }

  function toggleSelect() {
    selecting = !selecting;
    if (!selecting) selected = new Set();
  }

  function toggleNote(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id); else next.add(id);
    selected = next;
  }

  function sendSelected() {
    transferNoteIds = Array.from(selected);
  }

  const kindIcon: Record<string, string> = {
    checklist: "checklist", kanban: "view_kanban", table: "table_chart",
  };
  const kindColor: Record<string, string> = {
    checklist: "tertiary", kanban: "tertiary", table: "secondary",
  };
  const hintIcon: Record<string, string> = {
    plain: "description", markdown: "edit_note", code: "code",
  };
  const hintColor: Record<string, string> = {
    plain: "accent", markdown: "secondary", code: "secondary",
  };

  function noteIcon(note: { kind: string; content_hint?: string }) {
    if (note.kind === "document") return hintIcon[note.content_hint ?? ""] ?? "edit_note";
    return kindIcon[note.kind] ?? "description";
  }
  function noteColor(note: { kind: string; content_hint?: string }) {
    if (note.kind === "document") return hintColor[note.content_hint ?? ""] ?? "accent";
    return kindColor[note.kind] ?? "accent";
  }

  // toLocaleDateString builds a throwaway Intl.DateTimeFormat per call, and this
  // runs for every note older than a week on every refresh — same reasoning as
  // the shared Intl.Collator in stores/notes.ts.
  const dateFormat = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

  function formatRelative(unixSecs: number) {
    const HOUR = 3_600_000, DAY = 24 * HOUR;
    const diff = Date.now() - unixSecs * 1000;
    if (diff < HOUR) return Math.max(1, Math.round(diff / 60_000)) + "m";
    if (diff < DAY) return Math.round(diff / HOUR) + "h";
    if (diff < 7 * DAY) return Math.round(diff / DAY) + "d";
    return dateFormat.format(new Date(unixSecs * 1000));
  }

  const pinnedNotes = $derived(filtered.filter(n => n.pinned));
  const otherNotes = $derived(filtered.filter(n => !n.pinned));

  // Pick dark vs light ink for a note's custom background so text stays legible.
  function isLightColor(c: string): boolean {
    const hex = c.trim().match(/^#?([0-9a-f]{3}|[0-9a-f]{6})$/i);
    let r: number, g: number, b: number;
    if (hex) {
      let h = hex[1];
      if (h.length === 3) h = h.split("").map(x => x + x).join("");
      r = parseInt(h.slice(0, 2), 16); g = parseInt(h.slice(2, 4), 16); b = parseInt(h.slice(4, 6), 16);
    } else {
      const m = c.match(/(\d+)[,\s]+(\d+)[,\s]+(\d+)/);
      if (!m) return true;
      r = +m[1]; g = +m[2]; b = +m[3];
    }
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.6;
  }

  // Keyed on the image itself, never the note id: a note that swaps its
  // background needs a fresh reading, and notes sharing an image cost one decode.
  let imgInk = $state<Record<string, "dark" | "light">>({});
  const analyzing = new Set<string>();

  // Results land one image at a time but are published together. Spreading the
  // map per image is O(k^2) allocation across k images, and every write
  // invalidates each card's ink, so a library of backgrounds re-rendered the
  // whole list once per image while it loaded.
  let settled: Record<string, "dark" | "light"> = {};
  let flushQueued = false;
  function record(url: string, ink: "dark" | "light") {
    settled[url] = ink;
    if (flushQueued) return;
    flushQueued = true;
    queueMicrotask(() => {
      flushQueued = false;
      imgInk = { ...imgInk, ...settled };
      settled = {};
    });
  }

  function analyzeImage(url: string) {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      try {
        const c = document.createElement("canvas");
        c.width = 16; c.height = 16;
        const ctx = c.getContext("2d")!;
        ctx.drawImage(img, 0, 0, 16, 16);
        const d = ctx.getImageData(0, 0, 16, 16).data;
        let sum = 0;
        for (let i = 0; i < d.length; i += 4) sum += 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
        record(url, sum / (d.length / 4) / 255 > 0.6 ? "dark" : "light");
      } catch { record(url, "dark"); }
    };
    img.onerror = () => record(url, "dark");
    img.src = url;
  }

  /// Backgrounds no longer ride in the list payload, so a note's image comes
  /// from the separately-cached map instead of the row.
  const bgOf = (id: string): string | undefined => $bgImages[id];

  $effect(() => {
    const live = new Set(Object.values($bgImages).filter((u): u is string => !!u));
    for (const url of live) {
      if (url in imgInk || analyzing.has(url)) continue;
      analyzing.add(url);
      analyzeImage(url);
    }
    // Forget images no note carries any more, so the cache tracks the library
    // instead of growing for the life of the session.
    const stale = Object.keys(imgInk).filter(u => !live.has(u));
    if (stale.length) {
      for (const u of stale) analyzing.delete(u);
      imgInk = Object.fromEntries(Object.entries(imgInk).filter(([u]) => live.has(u)));
    }
  });

  function cardInk(note: { id: string; bg_color?: string }): "dark" | "light" | null {
    if (note.bg_color) return isLightColor(note.bg_color) ? "dark" : "light";
    const url = bgOf(note.id);
    if (url) return imgInk[url] ?? "dark";
    return null;
  }
</script>

<div class="page" class:desktop>
  {#if desktop}
    <div class="pane-head">
      <button class="menu-btn" onclick={() => $sidebarOpen = true} aria-label="Open menu">
        <span class="material-symbols-outlined">menu</span>
      </button>
      <span class="wordmark">Panote</span>
      <button class="compose-btn" onclick={() => showNewNote = true} aria-label="New note">
        <span class="material-symbols-outlined">add</span>
      </button>
    </div>
  {/if}
  <div class="toolbar">
    {#if !desktop}
      <button class="menu-btn" onclick={() => $sidebarOpen = true} aria-label="Open menu">
        <span class="material-symbols-outlined">menu</span>
      </button>
    {/if}
    <div class="search-wrap">
      <span class="material-symbols-outlined search-icon">search</span>
      <input class="search" placeholder="Search notes" bind:value={filter} />
      {#if filter}
        <button class="clear-btn" onclick={() => filter = ""} aria-label="Clear search">
          <span class="material-symbols-outlined" style="font-size: 18px;">close</span>
        </button>
      {/if}
    </div>
    <div class="sort-wrap">
      <button class="sort-btn" onclick={() => sortOpen = !sortOpen} aria-label="Sort notes">
        <span class="material-symbols-outlined">swap_vert</span>
      </button>
      {#if sortOpen}
        <div class="sort-backdrop" role="presentation" onclick={() => sortOpen = false}></div>
        <div class="sort-dropdown">
          {#each sortOptions as opt}
            <button class="sort-option" class:active={$sortPref.field === opt.field}
              onclick={() => sortPref.update(c => ({ field: opt.field, dir: c.dir }))}>
              <span class="material-symbols-outlined" style="font-size: 18px;">{$sortPref.field === opt.field ? "radio_button_checked" : "radio_button_unchecked"}</span>
              <span>{opt.label}</span>
            </button>
          {/each}
          <div class="sort-divider"></div>
          <button class="sort-option"
            onclick={() => sortPref.update(c => ({ field: c.field, dir: c.dir === "asc" ? "desc" : "asc" }))}>
            <span class="material-symbols-outlined" style="font-size: 18px;">{$sortPref.dir === "asc" ? "arrow_upward" : "arrow_downward"}</span>
            <span>{$sortPref.dir === "asc" ? "Ascending" : "Descending"}</span>
          </button>
        </div>
      {/if}
    </div>
    <button class="select-btn" class:active={selecting} onclick={toggleSelect} aria-label={selecting ? "Cancel selection" : "Select notes"}>
      <span class="material-symbols-outlined">{selecting ? "check_box" : "checklist_rtl"}</span>
    </button>
  </div>

  {#snippet noteCard(note: NoteMetadata)}
      <li>
        {#if selecting}
          <button
            class="note-card"
            class:checked={selected.has(note.id)}
            onclick={() => toggleNote(note.id)}
          >
            <span class="select-box" class:on={selected.has(note.id)}>
              {#if selected.has(note.id)}
                <span class="material-symbols-outlined" style="font-size: 20px; font-variation-settings: 'wght' 700;">check</span>
              {/if}
            </span>
            <div class="note-info">
              <strong>{note.title || "Untitled"}</strong>
              <div class="tags">
                {#each note.tags.slice(0, 3) as tag}<span class="tag">#{tag}</span>{/each}
              </div>
            </div>
            <span class="date">{formatRelative(note.updated_at)}</span>
          </button>
        {:else}
          <div class="note-card"
            role="button" tabindex="0"
            class:active={desktop && note.id === activeId}
            class:dark-ink={cardInk(note) === "dark"}
            class:light-ink={cardInk(note) === "light"}
            class:has-bg-image={bgOf(note.id)}
            style:background-color={note.bg_color ?? undefined}
            style:background-image={safeBgImageUrl(bgOf(note.id))}
            onclick={() => goto(`/note/${note.id}`)}
            onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); goto(`/note/${note.id}`); } }}
          >
            <span class="badge-wrap">
              <span class="kind-badge {noteColor(note)}">
                <span class="material-symbols-outlined">{noteIcon(note)}</span>
              </span>
              {#if note.pinned}
                <span class="pin-indicator"><span class="material-symbols-outlined" style="font-size: 12px; font-variation-settings: 'FILL' 1;">push_pin</span></span>
              {/if}
            </span>
            <div class="note-info">
              <div class="title-row">
                <strong>{note.title || "Untitled"}</strong>
                {#if note.has_note_password}<span class="lock"><span class="material-symbols-outlined" style="font-size: 14px;">lock</span></span>{/if}
              </div>
              <!-- Both slots always render, empty when there is nothing to
                   show. Conditional slots made card height follow content, so a
                   bare note sat noticeably shorter than one with a preview and
                   tags. Reserving them keeps every card the same height without
                   hard-coding one. -->
              <p class="preview-text">
                {#if note.show_preview}
                  {#if note.has_note_password}Locked note{:else if note.preview_text}{note.preview_text}{/if}
                {/if}
              </p>
              <div class="tags">
                {#each note.tags.slice(0, 3) as tag}<span class="tag">#{tag}</span>{/each}
              </div>
            </div>
            <div class="trailing">
              <span class="date">{formatRelative(note.updated_at)}</span>
              <button class="card-menu" aria-label="More options" onclick={(e) => { e.stopPropagation(); menuNoteId = menuNoteId === note.id ? null : note.id; }}>
                <span class="material-symbols-outlined">more_vert</span>
              </button>
            </div>
          </div>
          {#if menuNoteId === note.id}
            <div class="card-menu-backdrop" role="presentation" onclick={(e) => { e.stopPropagation(); menuNoteId = null; }}></div>
            <div class="card-popover">
              <button class="popover-item" onclick={(e) => { e.stopPropagation(); menuNoteId = null; togglePin(note.id, note.pinned); }}>
                <span class="material-symbols-outlined" style="font-size: 18px;">{note.pinned ? "push_pin" : "push_pin"}</span>
                {note.pinned ? "Unpin" : "Pin"}
              </button>
              <button class="popover-item" onclick={(e) => { e.stopPropagation(); menuNoteId = null; goto(`/note/${note.id}?mode=view`); }}>
                <span class="material-symbols-outlined" style="font-size: 18px;">visibility</span>
                View
              </button>
              <button class="popover-item" onclick={(e) => { e.stopPropagation(); menuNoteId = null; goto(`/note/${note.id}?mode=edit`); }}>
                <span class="material-symbols-outlined" style="font-size: 18px;">edit</span>
                Edit
              </button>
              {#if note.has_note_password}
                <button class="popover-item" onclick={(e) => { e.stopPropagation(); menuNoteId = null; pwModal = { mode: "change", ids: [note.id], isBatch: false }; }}>
                  <span class="material-symbols-outlined" style="font-size: 18px;">password</span>
                  Change password
                </button>
                <button class="popover-item danger" onclick={(e) => { e.stopPropagation(); menuNoteId = null; pwModal = { mode: "remove", ids: [note.id], isBatch: false }; }}>
                  <span class="material-symbols-outlined" style="font-size: 18px;">lock_open</span>
                  Remove password
                </button>
              {:else}
                <button class="popover-item" onclick={(e) => { e.stopPropagation(); menuNoteId = null; pwModal = { mode: "set", ids: [note.id], isBatch: false }; }}>
                  <span class="material-symbols-outlined" style="font-size: 18px;">lock</span>
                  Set password
                </button>
              {/if}
              <button class="popover-item danger" onclick={(e) => { e.stopPropagation(); menuNoteId = null; doDelete(note.id); }}>
                <span class="material-symbols-outlined" style="font-size: 18px;">delete</span>
                Delete
              </button>
            </div>
          {/if}
        {/if}
      </li>
  {/snippet}

  <ul class="note-list">
    {#if pinnedNotes.length}
      <li class="section-label">
        <span class="material-symbols-outlined sec-ico" style="font-size: 15px; font-variation-settings: 'FILL' 1;">push_pin</span>
        <span>Pinned</span>
      </li>
      {#each pinnedNotes as note (note.id)}{@render noteCard(note)}{/each}
    {/if}
    {#if pinnedNotes.length && otherNotes.length}
      <li class="section-label"><span>All notes</span></li>
    {/if}
    {#each otherNotes as note (note.id)}{@render noteCard(note)}{/each}
    {#if filtered.length === 0}
      <li class="empty">
        <span class="material-symbols-outlined empty-icon">{filter ? "search_off" : "note_add"}</span>
        <span>{filter ? "No notes match your search." : desktop ? "No notes yet. Use + to create one." : "No notes yet. Tap + to create one."}</span>
      </li>
    {/if}
    {#if hiddenLocked}
      <li class="locked-hint">
        <span class="material-symbols-outlined" style="font-size: 14px;">lock</span>
        <span>{hiddenLocked} locked {hiddenLocked === 1 ? "note" : "notes"} hidden</span>
      </li>
    {/if}
    {#if notShown > 0}
      <li class="locked-hint">
        <span class="material-symbols-outlined" style="font-size: 14px;">more_horiz</span>
        <span>Showing {$notes.length} of {$totalNotes} notes</span>
      </li>
    {/if}
  </ul>
</div>

<!-- FAB — touch layout only; desktop composes from the pane header. -->
{#if !desktop}
  {#if fabOpen}
    <div class="fab-backdrop" role="presentation" onclick={() => fabOpen = false}></div>
  {/if}
  <div class="fab-wrap">
    {#if fabOpen}
      <div class="fab-options">
        {#each fabKinds as kind, i}
          <button class="fab-option" style="animation-delay: {(fabKinds.length - 1 - i) * 40}ms"
            onclick={() => { fabOpen = false; goto(`/note/new?kind=${kind.id}`); }}>
            <span class="fab-label">{kind.label}</span>
            <span class="fab-badge"><span class="material-symbols-outlined">{kind.icon}</span></span>
          </button>
        {/each}
      </div>
    {/if}
    <button class="fab" class:open={fabOpen} onclick={() => fabOpen = !fabOpen} aria-label="Create note">
      <span class="material-symbols-outlined">add</span>
    </button>
  </div>
{/if}

{#if selecting && selected.size > 0}
  <div class="action-bar" class:desktop>
    <button class="bar-cancel" onclick={toggleSelect} aria-label="Cancel selection">
      <span class="material-symbols-outlined">close</span>
    </button>
    <span class="sel-count">{selected.size} selected</span>
    <div class="bar-spacer"></div>
    <button class="btn-ghost" onclick={() => pwModal = { mode: "set", ids: Array.from(selected), isBatch: true }} aria-label="Protect selected">
      <span class="material-symbols-outlined">lock</span>
    </button>
    <button class="btn-ghost" onclick={() => pwModal = { mode: "remove", ids: Array.from(selected), isBatch: true }} aria-label="Remove protection">
      <span class="material-symbols-outlined">lock_open</span>
    </button>
    <button class="btn-send" onclick={sendSelected}>Send</button>
  </div>
{/if}

{#if showNewNote}
  <NewNoteModal onclose={() => showNewNote = false} />
{/if}

{#if pwModal}
  <PasswordModal
    mode={pwModal.mode}
    title={pwHeading}
    onsubmit={handlePassword}
    onclose={() => pwModal = null}
  />
{/if}

{#if transferNoteIds}
  {#await import("$lib/components/TransferModal.svelte") then { default: TransferModal }}
    <TransferModal
      noteIds={transferNoteIds}
      onclose={() => { transferNoteIds = null; selecting = false; selected = new Set(); }}
    />
  {/await}
{/if}

{#if deleteTargetId}
  <ConfirmModal
    title="Delete note?"
    message="This note will be permanently deleted. This cannot be undone."
    confirmLabel="Delete"
    destructive
    onconfirm={confirmDelete}
    oncancel={() => deleteTargetId = null}
  />
{/if}

<style>
  .page {
    padding: 1.5rem 2rem 2rem;
    max-width: 600px; margin: 0 auto;
  }

  /* Toolbar */
  .toolbar {
    display: flex; gap: 6px; margin-bottom: 1.25rem; align-items: center;
    background: var(--surface-glass); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
    padding: 0.4rem 0.5rem; border-radius: var(--radius);
    border: 1px solid var(--border);
    box-shadow: 0 2px 12px var(--shadow-color);
    position: relative; z-index: 10;
  }
  .menu-btn, .sort-btn, .select-btn {
    width: 40px; height: 40px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    border-radius: var(--radius-full); transition: all 0.15s ease;
  }
  .menu-btn:hover, .sort-btn:hover, .select-btn:hover { color: var(--accent); background: var(--accent-muted); }
  .toolbar button > .material-symbols-outlined { font-size: 22px; }
  .search-wrap {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; height: 38px;
    background: var(--surface-container); border-radius: var(--radius-full);
    padding: 0 0.7rem; transition: box-shadow 0.15s ease;
  }
  .search-wrap:focus-within { box-shadow: 0 0 0 2px var(--accent-muted); }
  .search-icon { color: var(--muted); font-size: 18px; flex-shrink: 0; }
  .search {
    flex: 1; min-width: 0; height: 100%; padding: 0;
    border: none; outline: none; background: transparent; color: var(--text);
    font-size: 0.9rem;
  }
  .search::placeholder { color: var(--muted); }
  .clear-btn {
    display: flex; align-items: center; flex-shrink: 0;
    background: none; border: none; cursor: pointer; color: var(--muted);
    padding: 2px; border-radius: var(--radius-full); transition: all 0.15s ease;
  }
  .clear-btn:hover { color: var(--accent); background: var(--accent-muted); }

  /* Sort */
  .sort-wrap { position: relative; flex-shrink: 0; z-index: 20; }
  .sort-btn.active { color: var(--accent); background: var(--accent-muted); }
  .sort-backdrop { position: fixed; inset: 0; z-index: 19; }
  .sort-dropdown {
    position: absolute; top: calc(100% + 0.5rem); right: 0; z-index: 20;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.3rem; min-width: 170px;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
  }
  .sort-option {
    width: 100%; display: flex; align-items: center; justify-content: flex-start; gap: 10px;
    padding: 0.5rem 0.6rem; border: none; background: none;
    border-radius: var(--radius-sm); cursor: pointer; text-align: left;
    font-size: 0.85rem; color: var(--text-secondary);
    transition: all 0.1s ease;
  }
  .sort-option:hover { background: var(--hover); color: var(--text); }
  .sort-option.active { color: var(--accent); font-weight: 700; }
  .sort-divider { height: 1px; background: var(--border); margin: 4px 6px; }

  .select-btn.active { color: var(--accent); background: var(--accent-muted); }
  @media (max-width: 640px) {
    .page:not(.desktop) { padding: 1rem 0.75rem calc(1rem + env(safe-area-inset-bottom, 0px)); }
    .search { font-size: 0.85rem; }
  }

  /* Note list */
  .note-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.6rem; }
  li { display: flex; align-items: center; min-width: 0; position: relative; }
  .note-card {
    flex: 1; min-width: 0; display: flex; align-items: flex-start; gap: 0.8rem;
    padding: 0.85rem 0.95rem; border-radius: var(--radius);
    /* background-color, never the `background` shorthand: these cards can carry
       a user image, and the shorthand resets background-position/size, which
       knocks a centred cover image back to top-left at its natural size. */
    border: 1px solid transparent; background-color: var(--surface);
    text-decoration: none; color: var(--text);
    cursor: pointer; width: 100%; text-align: left;
    box-shadow: 0 4px 16px var(--shadow-color);
    transition: all 0.2s ease;
  }
  .note-card:hover {
    background-color: var(--hover);
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    transform: translateY(-2px);
    border-color: var(--accent-muted);
  }
  .note-card:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--accent), 0 4px 16px var(--shadow-color);
  }
  .note-card.checked { border-color: var(--accent); background-color: var(--accent-muted); }

  .trailing {
    display: flex; flex-direction: column; align-items: flex-end; justify-content: space-between;
    align-self: stretch; flex-shrink: 0; gap: 0.4rem;
  }
  .title-row { display: flex; align-items: center; gap: 6px; min-width: 0; }

  .badge-wrap { position: relative; flex-shrink: 0; }
  .pin-indicator {
    position: absolute; top: -4px; right: -4px;
    background: var(--accent); color: var(--on-accent);
    width: 18px; height: 18px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    box-shadow: 0 1px 4px var(--shadow-color);
    pointer-events: none;
  }
  .select-box {
    width: 40px; height: 40px; border-radius: 12px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    border: 2px solid var(--border); background: transparent; color: var(--on-accent);
    transition: all 0.15s ease;
  }
  .select-box.on { border-color: var(--accent); background: var(--accent); }

  .kind-badge {
    width: 40px; height: 40px; border-radius: 12px;
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0; font-size: 1rem;
  }
  .kind-badge.accent { background: var(--accent-surface); color: var(--accent); }
  .kind-badge.secondary { background: var(--secondary-surface); color: var(--secondary); }
  .kind-badge.tertiary { background: var(--tertiary-surface); color: var(--tertiary); }

  .note-info { flex: 1; min-width: 0; overflow: hidden; }
  .note-info strong {
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-weight: 700; font-size: 0.95rem;
  }
  .lock { color: var(--muted); display: flex; flex-shrink: 0; }
  /* Always occupies two clamped lines, so a one-line preview, a two-line one and
     an empty one are the same height. Height comes from the type, not a magic
     number, so it survives a font-size change. */
  .preview-text {
    margin: 3px 0 0; font-size: 0.78rem; color: var(--muted); line-height: 1.4;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical;
    overflow: hidden;
    min-height: calc(2 * 1.4 * 0.78rem);
  }
  /* One row, never two: a fourth tag is clipped rather than growing the card. */
  .tags {
    display: flex; gap: 6px; margin-top: 8px; flex-wrap: nowrap;
    overflow: hidden; min-height: 1.15rem;
  }
  .tag {
    font-size: 0.68rem; padding: 2px 10px; font-weight: 600;
    background: var(--accent-muted); border-radius: var(--radius-full); color: var(--accent);
  }
  .date { font-size: 0.72rem; color: var(--muted); white-space: nowrap; font-weight: 500; }

  /* Auto-contrast ink for notes with a custom background */
  .note-card.dark-ink { color: #2e1a28; }
  .note-card.dark-ink .date,
  .note-card.dark-ink .preview-text,
  .note-card.dark-ink .lock,
  .note-card.dark-ink .card-menu { color: #604868; }
  .note-card.dark-ink .tag { background: rgba(0,0,0,0.08); color: #604868; }

  .note-card.light-ink { color: #ffffff; }
  .note-card.light-ink .date,
  .note-card.light-ink .preview-text,
  .note-card.light-ink .lock,
  .note-card.light-ink .card-menu { color: rgba(255,255,255,0.88); }
  .note-card.light-ink .tag { background: rgba(255,255,255,0.22); color: #ffffff; }

  .note-card.has-bg-image {
    background-size: cover; background-position: center center;
    background-repeat: no-repeat;
    position: relative;
  }
  .note-card > * { position: relative; z-index: 1; }
  .note-card.has-bg-image::before {
    content: ""; position: absolute; inset: 0; z-index: 0;
    border-radius: inherit; pointer-events: none;
  }
  .note-card.has-bg-image.dark-ink::before { background: rgba(255,255,255,0.45); }
  .note-card.has-bg-image.light-ink::before { background: rgba(0,0,0,0.4); }

  .section-label {
    display: flex; align-items: center; gap: 6px;
    padding: 0.6rem 0.5rem 0.1rem;
    font-size: 0.72rem; font-weight: 700; letter-spacing: 0.04em;
    text-transform: uppercase; color: var(--muted);
  }
  .section-label .sec-ico { color: var(--accent); }

  /* Card context menu */
  .card-menu {
    background: none; border: none; cursor: pointer; margin: 0 -2px -2px 0;
    color: var(--muted); flex-shrink: 0; padding: 0.2rem;
    border-radius: var(--radius-full); transition: all 0.15s ease;
    display: flex; align-items: center;
  }
  .card-menu:hover { color: var(--text); background: var(--hover); }
  .card-menu-backdrop { position: fixed; inset: 0; z-index: 19; }
  .card-popover {
    position: absolute; right: 0; top: 100%; z-index: 20;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.25rem; box-shadow: 0 8px 24px var(--shadow-color-hover);
    min-width: 140px;
  }
  .popover-item {
    width: 100%; display: flex; align-items: center; gap: 0.5rem;
    padding: 0.5rem 0.75rem; border: none; background: none;
    border-radius: var(--radius-sm); cursor: pointer; font-size: 0.85rem;
    color: var(--text);
    transition: background 0.1s ease;
  }
  .popover-item:hover { background: var(--hover); }
  .popover-item.danger { color: var(--error); }

  /* Quiet footnote, not a warning: the exclusion is intentional. */
  .locked-hint {
    justify-content: center; gap: 6px;
    padding: 0.7rem 0.5rem 0.2rem;
    color: var(--muted); font-size: 0.75rem; font-weight: 500;
  }

  .empty {
    color: var(--muted); padding: 3rem; text-align: center; font-weight: 500;
    display: flex; flex-direction: column; align-items: center; gap: 0.75rem;
  }
  .empty-icon { font-size: 48px; opacity: 0.4; }

  /* Action bar */
  .action-bar {
    position: fixed; left: 0; right: 0; z-index: 60;
    bottom: env(safe-area-inset-bottom, 0px);
    background: var(--surface-glass); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
    border-top: 1px solid var(--border);
    padding: 0.8rem 1rem;
    display: flex; align-items: center; gap: 0.6rem;
    box-shadow: 0 -4px 16px var(--shadow-color);
  }
  .bar-cancel {
    width: 40px; height: 40px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    border-radius: var(--radius-full); transition: all 0.15s ease;
  }
  .bar-cancel:hover { color: var(--accent); background: var(--accent-muted); }
  .sel-count { font-size: 0.9rem; color: var(--text); font-weight: 700; }
  .bar-spacer { flex: 1; }
  .btn-ghost {
    width: 40px; height: 40px; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--text-secondary); cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .btn-ghost:hover { border-color: var(--accent); color: var(--accent); background: var(--accent-muted); }
  .btn-ghost .material-symbols-outlined { font-size: 20px; }
  .btn-send {
    padding: 0.6rem 1.4rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; cursor: pointer;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease;
  }
  .btn-send:hover { transform: scale(1.03); }

  /* FAB */
  .fab-backdrop { position: fixed; inset: 0; z-index: 70; background: rgba(0,0,0,0.25); }
  .fab-wrap {
    position: fixed; right: 1.5rem; bottom: calc(1.5rem + env(safe-area-inset-bottom, 0px));
    z-index: 80; display: flex; flex-direction: column; align-items: flex-end; gap: 0.75rem;
  }
  .fab {
    width: 56px; height: 56px; border-radius: var(--radius-full); border: none;
    background: var(--accent); color: var(--on-accent); cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    box-shadow: 0 6px 20px var(--shadow-color-hover);
    transition: transform 0.2s ease, background 0.15s ease;
  }
  .fab .material-symbols-outlined { font-size: 28px; transition: transform 0.25s ease; }
  .fab.open .material-symbols-outlined { transform: rotate(45deg); }
  .fab:hover { transform: scale(1.08); }
  .fab-options { display: flex; flex-direction: column; align-items: flex-end; gap: 0.5rem; }
  .fab-option {
    display: flex; align-items: center; gap: 0.6rem; padding: 0.5rem 0.55rem 0.5rem 1rem;
    border: 1px solid var(--border); border-radius: var(--radius-full);
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    color: var(--text); cursor: pointer; font-size: 0.85rem; font-weight: 600;
    box-shadow: 0 4px 16px var(--shadow-color);
    animation: fab-pop 0.2s ease both;
    transition: background 0.1s ease, transform 0.1s ease;
    white-space: nowrap;
  }
  .fab-option:hover { background: var(--hover); transform: translateX(-4px); }
  .fab-label { line-height: 1; }
  .fab-badge {
    width: 34px; height: 34px; border-radius: var(--radius-full);
    background: var(--accent-muted); color: var(--accent);
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
  }
  .fab-badge .material-symbols-outlined { font-size: 18px; }
  @keyframes fab-pop {
    from { opacity: 0; transform: translateY(8px) scale(0.9); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  /* ---- Desktop pane variant ----
     A 320px column reads as a list, not a deck of floating cards: flat rows,
     hairline separators, tinted active row. Touch layout above is untouched. */
  .page.desktop {
    max-width: none; margin: 0;
    padding: 0.75rem 0.6rem 1rem;
  }
  .pane-head {
    display: flex; align-items: center; gap: 0.35rem;
    padding: 0 0.25rem 0.5rem;
  }
  .wordmark {
    flex: 1; min-width: 0;
    font-size: 1.05rem; font-weight: 900; letter-spacing: -0.02em; color: var(--text);
  }
  .compose-btn {
    width: 34px; height: 34px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    border: none; border-radius: var(--radius-full); cursor: pointer;
    background: var(--accent); color: var(--on-accent);
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease, background 0.15s ease;
  }
  .compose-btn:hover { background: var(--accent-hover); transform: scale(1.06); }
  .compose-btn:active { transform: scale(0.95); }
  .compose-btn .material-symbols-outlined { font-size: 22px; }

  .page.desktop .toolbar {
    margin-bottom: 0.5rem;
    background: none; backdrop-filter: none; -webkit-backdrop-filter: none;
    border: none; box-shadow: none;
    padding: 0 0.25rem;
  }
  /* Desktop stays a little denser than touch, but it is still a list of cards:
     `gap: 0` with hairline separators made the same list look unlike the phone.
     Cards get real spacing, so the separators are gone with it. */
  .page.desktop .note-list { gap: 0.4rem; }
  .page.desktop .note-card {
    border-radius: var(--radius);
    box-shadow: 0 2px 8px var(--shadow-color);
    padding: 0.7rem;
    gap: 0.7rem;
  }
  /* No lift on hover: rows nudging themselves upward reads as jitter in a
     dense list you are scanning with a mouse. */
  .page.desktop .note-card:hover {
    transform: none;
    box-shadow: 0 4px 14px var(--shadow-color-hover);
    background-color: var(--hover);
  }
  .page.desktop .note-card.active {
    background: var(--accent-muted); border-color: var(--accent-muted);
  }
  .page.desktop .note-card.active strong { color: var(--accent); }
  .page.desktop .kind-badge, .page.desktop .select-box { width: 34px; height: 34px; border-radius: 10px; }
  .page.desktop .kind-badge .material-symbols-outlined { font-size: 19px; }
  .page.desktop .note-info strong { font-size: 0.88rem; }
  .page.desktop .preview-text { font-size: 0.75rem; -webkit-line-clamp: 1; line-clamp: 1; }
  .page.desktop .trailing { flex-direction: row-reverse; align-items: center; align-self: flex-start; }
  .page.desktop .empty { padding: 2rem 1rem; font-size: 0.85rem; }

  /* Scoped to the list column instead of spanning the whole window. */
  .action-bar.desktop {
    position: sticky; left: auto; right: auto;
    bottom: 0; margin-top: 0.5rem;
    border-radius: var(--radius); border: 1px solid var(--border);
    padding: 0.6rem 0.7rem; gap: 0.4rem;
  }
  .action-bar.desktop .sel-count { font-size: 0.8rem; }
  .action-bar.desktop .btn-ghost, .action-bar.desktop .bar-cancel { width: 34px; height: 34px; }
  .action-bar.desktop .btn-send { padding: 0.45rem 0.9rem; font-size: 0.85rem; }
</style>
