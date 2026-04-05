<script lang="ts">
  import { onMount } from "svelte";
  import { notes, refreshNotes } from "$lib/stores/notes";
  import { noteDelete, getDeviceName, setDeviceName, startReceiving, stopReceiving, isReceiving } from "$lib/tauri";
  import { goto } from "$app/navigation";
  import { getVersion } from "@tauri-apps/api/app";
  import TransferModal from "$lib/components/TransferModal.svelte";

  let filter = $state("");
  let creating = $state(false);
  let appVersion = $state("");
  let newKind = $state<"text" | "markdown" | "checklist" | "code" | "kanban">("text");
  let sidebarOpen = $state(false);

  let selecting = $state(false);
  let selected = $state(new Set<string>());
  let transferNoteIds = $state<string[] | null>(null);
  let deviceName = $state("");
  let editingName = $state(false);
  let nameInput = $state("");
  let receiving = $state(false);

  onMount(async () => {
    await refreshNotes();
    appVersion = await getVersion();
    try { deviceName = await getDeviceName(); } catch {}
    try { receiving = await isReceiving(); } catch {}
  });

  async function saveName() {
    const trimmed = nameInput.trim();
    if (!trimmed || trimmed === deviceName) { editingName = false; return; }
    try {
      await setDeviceName(trimmed);
      deviceName = trimmed;
    } catch {}
    editingName = false;
  }

  const filtered = $derived(
    $notes.filter(n =>
      n.title.toLowerCase().includes(filter.toLowerCase()) ||
      n.tags.some(t => t.toLowerCase().includes(filter.toLowerCase()))
    )
  );

  async function doDelete(id: string) {
    if (!confirm("Delete this note?")) return;
    await noteDelete(id);
    await refreshNotes();
  }

  function startNew() {
    creating = true;
  }

  function goCreate() {
    creating = false;
    goto(`/note/new?kind=${newKind}`);
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

  async function toggleReceiving() {
    try {
      if (receiving) {
        await stopReceiving();
        receiving = false;
      } else {
        await startReceiving();
        receiving = true;
      }
    } catch {}
  }

  const kindIcon: Record<string, string> = {
    text: "📄", markdown: "📝", checklist: "✅", code: "💻", kanban: "📋",
  };
  const kindLabel: Record<string, string> = {
    text: "Plain text", markdown: "Markdown", checklist: "Checklist", code: "Code", kanban: "Kanban",
  };
</script>

<div class="layout">
  <aside class="sidebar" class:open={sidebarOpen}>
    <div class="logo">Panote</div>
    <nav>
      <a href="/" class="nav-item active" onclick={() => sidebarOpen = false}>Notes</a>
    </nav>
    <div class="receive-toggle">
      <button
        class="toggle-btn"
        class:active={receiving}
        onclick={toggleReceiving}
        title={receiving ? "Receiving is ON — tap to disable" : "Enable receiving to accept transfers"}
      >
        <span class="toggle-dot"></span>
        <span class="toggle-label">{receiving ? "Receiving ON" : "Receiving OFF"}</span>
      </button>
    </div>
    {#if deviceName}
      <div class="device-name">
        {#if editingName}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="name-input"
            bind:value={nameInput}
            onkeydown={(e) => { if (e.key === "Enter") saveName(); if (e.key === "Escape") editingName = false; }}
            onblur={saveName}
            autofocus
          />
        {:else}
          <button class="name-btn" onclick={() => { nameInput = deviceName; editingName = true; }} title="Rename device">
            {deviceName}
          </button>
        {/if}
      </div>
    {/if}
    {#if appVersion}
      <div class="version">v{appVersion}</div>
    {/if}
  </aside>

  <div class="overlay" class:visible={sidebarOpen} role="button" tabindex="-1" onclick={() => sidebarOpen = false} onkeydown={() => {}}></div>

  <main class="content">
    <div class="toolbar">
      <button class="hamburger" onclick={() => sidebarOpen = !sidebarOpen} aria-label="Menu">☰</button>
      <input class="search" placeholder="Search notes or tags…" bind:value={filter} />
      <button class="select-btn" class:active={selecting} onclick={toggleSelect}>
        {selecting ? "Cancel" : "Select"}
      </button>
      {#if !selecting}
        <button class="new-btn" onclick={startNew}>+ New</button>
      {/if}
    </div>

    {#if creating}
      <div class="kind-picker">
        <label class="kind-label" for="kind-select">Note type</label>
        <select id="kind-select" class="kind-select" bind:value={newKind}>
          {#each ["text", "markdown", "checklist", "code", "kanban"] as k}
            <option value={k}>{kindIcon[k]} {kindLabel[k]}</option>
          {/each}
        </select>
        <div class="kind-actions">
          <button class="btn-create" onclick={goCreate}>Create</button>
          <button class="btn-cancel" onclick={() => creating = false}>Cancel</button>
        </div>
      </div>
    {/if}

    <ul class="note-list">
      {#each filtered as note (note.id)}
        <li>
          {#if selecting}
            <button
              class="note-card"
              class:checked={selected.has(note.id)}
              onclick={() => toggleNote(note.id)}
            >
              <span class="checkbox">{selected.has(note.id) ? "☑" : "☐"}</span>
              <span class="kind-badge">{kindIcon[note.kind] ?? "📄"}</span>
              <div class="note-info">
                <strong>{note.title}</strong>
                <div class="tags">
                  {#each note.tags as tag}<span class="tag">{tag}</span>{/each}
                </div>
              </div>
              <span class="date">{new Date(note.updated_at * 1000).toLocaleDateString()}</span>
            </button>
          {:else}
            <a href="/note/{note.id}" class="note-card">
              <span class="kind-badge">{kindIcon[note.kind] ?? "📄"}</span>
              <div class="note-info">
                <strong>{note.title}</strong>
                {#if note.has_note_password}<span class="lock">🔑</span>{/if}
                <div class="tags">
                  {#each note.tags as tag}<span class="tag">{tag}</span>{/each}
                </div>
              </div>
              <span class="date">{new Date(note.updated_at * 1000).toLocaleDateString()}</span>
            </a>
            <button class="del" onclick={() => doDelete(note.id)} title="Delete">🗑</button>
          {/if}
        </li>
      {/each}
      {#if filtered.length === 0}
        <li class="empty">No notes yet. Create one above.</li>
      {/if}
    </ul>
  </main>
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
  .layout { display: flex; height: 100%; overflow: hidden; }
  .sidebar {
    position: fixed; inset: 0 auto 0 0; z-index: 10;
    width: 220px;
    transform: translateX(-100%);
    transition: transform 0.2s ease;
    background: var(--surface); border-right: 1px solid var(--border);
    display: flex; flex-direction: column;
    padding: 1.5rem 1rem; gap: 1rem;
  }
  .sidebar.open { transform: translateX(0); }
  .logo { font-size: 1.2rem; font-weight: 700; }
  .name-btn {
    background: none; border: 1px dashed var(--border); border-radius: 6px;
    color: var(--muted); font-size: 0.78rem; padding: 0.3rem 0.5rem;
    cursor: pointer; width: 100%; text-align: left;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .name-btn:hover { color: var(--text); border-color: var(--text); }
  .name-input {
    width: 100%; padding: 0.3rem 0.5rem; font-size: 0.78rem;
    border: 1px solid var(--accent); border-radius: 6px;
    background: var(--input-bg); color: var(--text); outline: none;
  }
  .receive-toggle { margin-top: auto; }
  .toggle-btn {
    display: flex; align-items: center; gap: 0.5rem;
    width: 100%; padding: 0.45rem 0.6rem; border-radius: 8px;
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-size: 0.8rem;
    transition: all 0.15s ease;
  }
  .toggle-btn:hover { border-color: var(--text); color: var(--text); }
  .toggle-btn.active {
    border-color: #27ae60; color: #27ae60; background: rgba(39, 174, 96, 0.08);
  }
  .toggle-dot {
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--muted); flex-shrink: 0;
    transition: background 0.15s ease;
  }
  .toggle-btn.active .toggle-dot { background: #27ae60; box-shadow: 0 0 4px rgba(39, 174, 96, 0.5); }
  .toggle-label { flex: 1; text-align: left; }
  .device-name { margin-top: 0.5rem; }
  .version { font-size: 0.72rem; color: var(--muted); }
  nav { display: flex; flex-direction: column; gap: 0.4rem; flex: 1; }
  .nav-item {
    padding: 0.5rem 0.75rem; border-radius: 6px;
    text-decoration: none; color: var(--text); font-size: 0.95rem;
  }
  .nav-item:hover, .nav-item.active { background: var(--hover); }
  .hamburger {
    background: none; border: none; cursor: pointer;
    font-size: 1.3rem; color: var(--text); padding: 0 0.25rem; flex-shrink: 0;
  }
  .overlay {
    display: none; position: fixed; inset: 0; z-index: 9;
    background: rgba(0,0,0,0.4);
  }
  .overlay.visible { display: block; }
  .content { flex: 1; overflow-y: auto; padding: 1.5rem 2rem; }
  @media (max-width: 640px) {
    .content { padding: 1rem 0.75rem; }
  }
  .toolbar { display: flex; gap: 0.75rem; margin-bottom: 1rem; align-items: center; flex-wrap: wrap; }
  .search {
    flex: 1; min-width: 0; padding: 0.6rem 1rem;
    border: 1px solid var(--border); border-radius: 8px;
    background: var(--input-bg); color: var(--text);
  }
  .new-btn {
    padding: 0.6rem 1.2rem; border-radius: 8px;
    border: none; background: var(--accent); color: #fff; font-weight: 600; cursor: pointer;
    flex-shrink: 0;
  }
  .select-btn {
    padding: 0.6rem 1rem; border-radius: 8px;
    border: 1px solid var(--border); background: transparent;
    color: var(--text); cursor: pointer; white-space: nowrap; flex-shrink: 0;
  }
  @media (max-width: 640px) {
    .search { padding: 0.5rem 0.75rem; font-size: 0.85rem; }
    .new-btn { padding: 0.5rem 0.75rem; font-size: 0.85rem; }
    .select-btn { padding: 0.5rem 0.7rem; font-size: 0.85rem; }
  }
  .select-btn.active { border-color: var(--accent); color: var(--accent); }
  .kind-picker {
    display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap;
    background: var(--surface); border-radius: 10px;
    padding: 0.75rem 1rem; margin-bottom: 1rem; border: 1px solid var(--border);
  }
  .kind-label { font-size: 0.85rem; color: var(--muted); white-space: nowrap; }
  .kind-select {
    flex: 1; min-width: 140px; padding: 0.5rem 0.75rem; border-radius: 7px;
    border: 1px solid var(--border); background: var(--input-bg);
    color: var(--text); font-size: 0.95rem; cursor: pointer;
  }
  .kind-actions { display: flex; gap: 0.5rem; margin-left: auto; }
  .btn-create {
    padding: 0.5rem 1.25rem; border-radius: 7px; border: none;
    background: var(--accent); color: #fff; font-weight: 600; cursor: pointer; font-size: 0.9rem;
  }
  .btn-cancel {
    padding: 0.5rem 1rem; border-radius: 7px;
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-size: 0.9rem;
  }
  .btn-cancel:hover { color: var(--text); border-color: var(--text); }
  .note-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.5rem; }
  li { display: flex; align-items: center; min-width: 0; }
  .note-card {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 0.75rem;
    padding: 0.75rem 1rem; border-radius: 8px;
    border: 1px solid var(--border); background: var(--surface);
    text-decoration: none; color: var(--text);
    cursor: pointer; width: 100%; text-align: left;
  }
  .note-card:hover { background: var(--hover); }
  .note-card.checked { border-color: var(--accent); background: var(--accent-muted); }
  .checkbox { font-size: 1.15rem; flex-shrink: 0; }
  .kind-badge { font-size: 1.3rem; }
  .note-info { flex: 1; min-width: 0; overflow: hidden; }
  .note-info strong { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .lock { font-size: 0.75rem; margin-left: 4px; }
  .tags { display: flex; gap: 4px; margin-top: 4px; flex-wrap: wrap; }
  .tag { font-size: 0.7rem; padding: 1px 6px; background: var(--accent-muted); border-radius: 12px; color: var(--accent); }
  .date { font-size: 0.8rem; color: var(--muted); white-space: nowrap; }
  .del { background: none; border: none; cursor: pointer; margin-left: 0.5rem; font-size: 1rem; opacity: 0.5; flex-shrink: 0; }
  .del:hover { opacity: 1; }
  .empty { color: var(--muted); padding: 2rem; text-align: center; }
  .action-bar {
    position: fixed; bottom: env(safe-area-inset-bottom, 0px); left: 0; right: 0; z-index: 50;
    background: var(--surface); border-top: 1px solid var(--border);
    padding: 0.85rem 1.5rem;
    display: flex; align-items: center; justify-content: space-between;
  }
  .sel-count { font-size: 0.9rem; color: var(--muted); }
  .btn-send {
    padding: 0.6rem 1.4rem; border-radius: 8px;
    border: none; background: var(--accent); color: #fff;
    font-weight: 600; cursor: pointer;
  }
</style>
