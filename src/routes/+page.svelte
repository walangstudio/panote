<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { notes, refreshNotes, sortPref, sortNotes, type SortField } from "$lib/stores/notes";
  import { noteDelete, notePin } from "$lib/tauri";
  import { sidebarOpen } from "$lib/stores/sidebar";
  import TransferModal from "$lib/components/TransferModal.svelte";

  let filter = $state("");

  let selecting = $state(false);
  let selected = $state(new Set<string>());
  let transferNoteIds = $state<string[] | null>(null);
  let sortOpen = $state(false);
  let menuNoteId = $state<string | null>(null);
  let fabOpen = $state(false);

  const fabKinds = [
    { id: "document", icon: "edit_note", label: "Document" },
    { id: "checklist", icon: "checklist", label: "Checklist" },
    { id: "kanban", icon: "view_kanban", label: "Kanban" },
    { id: "table", icon: "table_chart", label: "Table" },
  ] as const;

  const sortOptions: { field: SortField; label: string }[] = [
    { field: "updated", label: "Date modified" },
    { field: "created", label: "Date created" },
    { field: "title", label: "Title" },
    { field: "kind", label: "Type" },
  ];

  onMount(() => { refreshNotes(); });

  const filtered = $derived(
    sortNotes(
      $notes.filter(n =>
        n.title.toLowerCase().includes(filter.toLowerCase()) ||
        n.tags.some(t => t.toLowerCase().includes(filter.toLowerCase()))
      ),
      $sortPref
    )
  );

  async function doDelete(id: string) {
    if (!confirm("Delete this note?")) return;
    await noteDelete(id);
    await refreshNotes();
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

  function cycleSort(field: SortField) {
    sortPref.update(cur => ({
      field,
      dir: cur.field === field && cur.dir === "desc" ? "asc" : "desc",
    }));
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
</script>

<div class="page">
  <div class="toolbar">
    <button class="menu-btn" onclick={() => $sidebarOpen = true} aria-label="Open menu">
      <span class="material-symbols-outlined">menu</span>
    </button>
    <div class="search-wrap">
      <span class="material-symbols-outlined search-icon">search</span>
      <input class="search" placeholder="Search notes or tags…" bind:value={filter} />
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
              onclick={() => { cycleSort(opt.field); }}>
              <span>{opt.label}</span>
              {#if $sortPref.field === opt.field}
                <span class="material-symbols-outlined" style="font-size: 16px;">
                  {$sortPref.dir === "asc" ? "arrow_upward" : "arrow_downward"}
                </span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
    <button class="select-btn" class:active={selecting} onclick={toggleSelect}>
      {selecting ? "Cancel" : "Select"}
    </button>
  </div>

  <ul class="note-list">
    {#each filtered as note (note.id)}
      <li>
        {#if selecting}
          <button
            class="note-card"
            class:checked={selected.has(note.id)}
            onclick={() => toggleNote(note.id)}
          >
            <span class="checkbox">
              <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' {selected.has(note.id) ? 1 : 0};">
                {selected.has(note.id) ? "check_box" : "check_box_outline_blank"}
              </span>
            </span>
            <span class="kind-badge {noteColor(note)}">
              <span class="material-symbols-outlined">{noteIcon(note)}</span>
            </span>
            <div class="note-info">
              <strong>{note.title}</strong>
              <div class="tags">
                {#each note.tags as tag}<span class="tag">{tag}</span>{/each}
              </div>
            </div>
            <span class="date">{new Date(note.updated_at * 1000).toLocaleDateString()}</span>
          </button>
        {:else}
          <a href="/note/{note.id}" class="note-card"
            class:light-bg={note.bg_color || note.bg_image}
            class:has-bg-image={note.bg_image}
            style:background-color={note.bg_color ?? undefined}
            style:background-image={note.bg_image ? `url(${note.bg_image})` : undefined}
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
              <strong>{note.title}</strong>
              {#if note.has_note_password}<span class="lock"><span class="material-symbols-outlined" style="font-size: 14px;">lock</span></span>{/if}
              {#if note.show_preview && note.preview_text}
                <p class="preview-text">{note.preview_text}</p>
              {/if}
              <div class="tags">
                {#each note.tags as tag}<span class="tag">{tag}</span>{/each}
              </div>
            </div>
            <span class="date">{new Date(note.updated_at * 1000).toLocaleDateString()}</span>
          </a>
          <button class="card-menu" onclick={(e) => { e.preventDefault(); e.stopPropagation(); menuNoteId = menuNoteId === note.id ? null : note.id; }}>
            <span class="material-symbols-outlined">more_vert</span>
          </button>
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
              <button class="popover-item danger" onclick={(e) => { e.stopPropagation(); menuNoteId = null; doDelete(note.id); }}>
                <span class="material-symbols-outlined" style="font-size: 18px;">delete</span>
                Delete
              </button>
            </div>
          {/if}
        {/if}
      </li>
    {/each}
    {#if filtered.length === 0}
      <li class="empty">
        <span class="material-symbols-outlined empty-icon">note_add</span>
        <span>No notes yet. Tap + to create one.</span>
      </li>
    {/if}
  </ul>
</div>

<!-- FAB -->
{#if fabOpen}
  <div class="fab-backdrop" role="presentation" onclick={() => fabOpen = false}></div>
{/if}
<div class="fab-wrap">
  {#if fabOpen}
    <div class="fab-options">
      {#each fabKinds as kind, i}
        <button class="fab-option" style="animation-delay: {i * 40}ms"
          onclick={() => { fabOpen = false; goto(`/note/new?kind=${kind.id}`); }}>
          <span class="material-symbols-outlined">{kind.icon}</span>
          <span>{kind.label}</span>
        </button>
      {/each}
    </div>
  {/if}
  <button class="fab" class:open={fabOpen} onclick={() => fabOpen = !fabOpen} aria-label="Create note">
    <span class="material-symbols-outlined">add</span>
  </button>
</div>

{#if selecting && selected.size > 0}
  <div class="action-bar">
    <span class="sel-count">{selected.size} selected</span>
    <button class="btn-send" onclick={sendSelected}>Send selected</button>
  </div>
{/if}

{#if transferNoteIds}
  <TransferModal
    noteIds={transferNoteIds}
    onclose={() => { transferNoteIds = null; selecting = false; selected = new Set(); }}
  />
{/if}

<style>
  .page {
    padding: 1.5rem 2rem 2rem;
    max-width: 800px; margin: 0 auto;
  }

  /* Toolbar */
  .toolbar {
    display: flex; gap: 0.5rem; margin-bottom: 1.25rem; align-items: center;
    background: var(--surface-glass); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
    padding: 0.75rem 1rem; border-radius: var(--radius);
    border: 1px solid var(--border);
    box-shadow: 0 2px 12px var(--shadow-color);
    position: relative; z-index: 10;
  }
  .menu-btn {
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    display: flex; align-items: center; padding: 0.25rem; border-radius: var(--radius-full);
    flex-shrink: 0; transition: all 0.15s ease;
  }
  .menu-btn:hover { color: var(--accent); background: var(--accent-muted); }
  .search-wrap {
    flex: 1; min-width: 0; position: relative; display: flex; align-items: center;
  }
  .search-icon {
    position: absolute; left: 0.75rem; color: var(--muted); font-size: 20px;
    pointer-events: none;
  }
  .search {
    width: 100%; padding: 0.55rem 0.75rem 0.55rem 2.5rem;
    border: none; border-radius: var(--radius-full);
    background: var(--surface-container); color: var(--text);
    font-size: 0.9rem; outline: none;
    transition: box-shadow 0.15s ease;
  }
  .search:focus { box-shadow: 0 0 0 2px var(--accent-muted); }
  .search::placeholder { color: var(--muted); }

  /* Sort */
  .sort-wrap { position: relative; flex-shrink: 0; z-index: 20; }
  .sort-btn {
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    display: flex; align-items: center; padding: 0.35rem; border-radius: var(--radius-full);
    transition: all 0.15s ease;
  }
  .sort-btn:hover { color: var(--accent); background: var(--accent-muted); }
  .sort-backdrop { position: fixed; inset: 0; z-index: 19; }
  .sort-dropdown {
    position: absolute; top: calc(100% + 0.5rem); right: 0; z-index: 20;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.3rem; min-width: 170px;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
  }
  .sort-option {
    width: 100%; display: flex; align-items: center; justify-content: space-between;
    padding: 0.5rem 0.75rem; border: none; background: none;
    border-radius: var(--radius-sm); cursor: pointer;
    font-size: 0.85rem; color: var(--text-secondary);
    transition: all 0.1s ease;
  }
  .sort-option:hover { background: var(--hover); color: var(--text); }
  .sort-option.active { color: var(--accent); font-weight: 600; }

  .select-btn {
    padding: 0.5rem 1rem; border-radius: var(--radius-full);
    border: 1.5px solid var(--border); background: transparent;
    color: var(--text-secondary); cursor: pointer; white-space: nowrap; flex-shrink: 0;
    font-weight: 500; transition: all 0.15s ease;
  }
  .select-btn:hover { border-color: var(--accent); color: var(--accent); }
  .select-btn.active { border-color: var(--accent); color: var(--accent); background: var(--accent-muted); }
  @media (max-width: 640px) {
    .page { padding: 1rem 0.75rem calc(1rem + env(safe-area-inset-bottom, 0px)); }
    .toolbar { padding: 0.5rem 0.75rem; }
    .search { font-size: 0.85rem; padding: 0.45rem 0.6rem 0.45rem 2.2rem; }
    .select-btn { padding: 0.45rem 0.7rem; font-size: 0.85rem; }
  }

  /* Note list */
  .note-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.6rem; }
  li { display: flex; align-items: center; min-width: 0; position: relative; }
  .note-card {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 0.85rem;
    padding: 0.85rem 1.15rem; border-radius: var(--radius);
    border: 1px solid transparent; background: var(--surface);
    text-decoration: none; color: var(--text);
    cursor: pointer; width: 100%; text-align: left;
    box-shadow: 0 4px 16px var(--shadow-color);
    transition: all 0.2s ease;
  }
  .note-card:hover {
    background: var(--hover);
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    transform: translateY(-2px);
    border-color: var(--accent-muted);
  }
  .note-card.checked { border-color: var(--accent); background: var(--accent-muted); }

  .badge-wrap { position: relative; flex-shrink: 0; }
  .pin-indicator {
    position: absolute; top: -4px; right: -4px;
    background: var(--accent); color: var(--on-accent);
    width: 18px; height: 18px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    box-shadow: 0 1px 4px var(--shadow-color);
    pointer-events: none;
  }
  .checkbox { flex-shrink: 0; color: var(--accent); display: flex; align-items: center; }

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
    display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-weight: 700; font-size: 0.95rem;
  }
  .lock { margin-left: 4px; color: var(--muted); }
  .preview-text {
    margin: 2px 0 0; font-size: 0.78rem; color: var(--muted); line-height: 1.4;
    display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .tags { display: flex; gap: 6px; margin-top: 6px; flex-wrap: wrap; }
  .tag {
    font-size: 0.68rem; padding: 2px 10px; font-weight: 600;
    background: var(--accent-muted); border-radius: var(--radius-full); color: var(--accent);
  }
  .date { font-size: 0.78rem; color: var(--muted); white-space: nowrap; font-weight: 500; }
  .note-card.light-bg { color: #2e1a28; }
  .note-card.light-bg .date,
  .note-card.light-bg .preview-text { color: #604868; }
  .note-card.light-bg .tag { background: rgba(0,0,0,0.08); color: #604868; }
  .note-card.has-bg-image {
    background-size: cover; background-position: center;
    position: relative;
  }
  .note-card.has-bg-image::before {
    content: ""; position: absolute; inset: 0;
    background: rgba(255,255,255,0.55);
    border-radius: inherit; pointer-events: none;
  }

  /* Card context menu */
  .card-menu {
    background: none; border: none; cursor: pointer; margin-left: 0.15rem;
    color: var(--muted); flex-shrink: 0; padding: 0.3rem;
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
    padding: 0.85rem 1.5rem;
    display: flex; align-items: center; justify-content: space-between;
    box-shadow: 0 -4px 16px var(--shadow-color);
  }
  .sel-count { font-size: 0.9rem; color: var(--muted); font-weight: 500; }
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
    display: flex; align-items: center; gap: 0.6rem; padding: 0.55rem 1rem 0.55rem 0.75rem;
    border: 1px solid var(--border); border-radius: var(--radius-full);
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    color: var(--text); cursor: pointer; font-size: 0.85rem; font-weight: 500;
    box-shadow: 0 4px 16px var(--shadow-color);
    animation: fab-pop 0.2s ease both;
    transition: background 0.1s ease, transform 0.1s ease;
    white-space: nowrap;
  }
  .fab-option:hover { background: var(--hover); transform: translateX(-4px); }
  .fab-option .material-symbols-outlined { font-size: 20px; color: var(--accent); }
  @keyframes fab-pop {
    from { opacity: 0; transform: translateY(8px) scale(0.9); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }
</style>
