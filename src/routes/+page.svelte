<script lang="ts">
  import { onMount } from "svelte";
  import { notes, refreshNotes } from "$lib/stores/notes";
  import { noteDelete } from "$lib/tauri";
  import { goto } from "$app/navigation";
  import { getVersion } from "@tauri-apps/api/app";

  let filter = $state("");
  let creating = $state(false);
  let appVersion = $state("");
  let newKind = $state<"text" | "markdown" | "checklist" | "code" | "kanban">("text");
  let sidebarOpen = $state(false);

  onMount(async () => {
    await refreshNotes();
    appVersion = await getVersion();
  });

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

  const kindIcon: Record<string, string> = {
    text: "📄",
    markdown: "📝",
    checklist: "✅",
    code: "💻",
    kanban: "📋",
  };

  const kindLabel: Record<string, string> = {
    text: "Plain text",
    markdown: "Markdown",
    checklist: "Checklist",
    code: "Code",
    kanban: "Kanban",
  };
</script>

<div class="layout">
  <aside class="sidebar" class:open={sidebarOpen}>
    <div class="logo">Panote</div>
    <nav>
      <a href="/" class="nav-item active" onclick={() => sidebarOpen = false}>Notes</a>
      <a href="/transfer" class="nav-item" onclick={() => sidebarOpen = false}>Transfer</a>
    </nav>
    {#if appVersion}
      <div class="version">v{appVersion}</div>
    {/if}
  </aside>

  {#if sidebarOpen}
    <div class="overlay" role="button" tabindex="-1" onclick={() => sidebarOpen = false} onkeydown={() => {}}></div>
  {/if}

  <main class="content">
    <div class="toolbar">
      <button class="hamburger" onclick={() => sidebarOpen = !sidebarOpen} aria-label="Menu">☰</button>
      <input class="search" placeholder="Search notes or tags…" bind:value={filter} />
      <button class="new-btn" onclick={startNew}>+ New</button>
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
        </li>
      {/each}
      {#if filtered.length === 0}
        <li class="empty">No notes yet. Create one above.</li>
      {/if}
    </ul>
  </main>
</div>

<style>
  .layout { display: flex; height: 100vh; overflow: hidden; }
  .sidebar {
    width: 200px; flex-shrink: 0;
    background: var(--surface);
    border-right: 1px solid var(--border);
    display: flex; flex-direction: column;
    padding: 1.5rem 1rem;
    gap: 1rem;
  }
  .logo { font-size: 1.2rem; font-weight: 700; }
  .version { font-size: 0.72rem; color: var(--muted); margin-top: auto; }
  nav { display: flex; flex-direction: column; gap: 0.4rem; flex: 1; }
  .nav-item {
    padding: 0.5rem 0.75rem; border-radius: 6px;
    text-decoration: none; color: var(--text);
    font-size: 0.95rem;
  }
  .nav-item:hover, .nav-item.active { background: var(--hover); }
  .hamburger {
    display: none;
    background: none; border: none; cursor: pointer;
    font-size: 1.3rem; color: var(--text); padding: 0 0.25rem;
    flex-shrink: 0;
  }
  .overlay {
    display: none;
    position: fixed; inset: 0; z-index: 9;
    background: rgba(0,0,0,0.4);
  }
  @media (max-width: 640px) {
    .hamburger { display: block; }
    .sidebar {
      position: fixed; inset: 0 auto 0 0; z-index: 10;
      transform: translateX(-100%);
      transition: transform 0.2s ease;
      width: 220px;
    }
    .sidebar.open { transform: translateX(0); }
    .overlay { display: block; }
  }
  .content { flex: 1; overflow-y: auto; padding: 1.5rem 2rem; }
  .toolbar { display: flex; gap: 0.75rem; margin-bottom: 1rem; align-items: center; }
  .search {
    flex: 1; padding: 0.6rem 1rem;
    border: 1px solid var(--border); border-radius: 8px;
    background: var(--input-bg); color: var(--text);
  }
  .new-btn {
    padding: 0.6rem 1.2rem; border-radius: 8px;
    border: none; background: var(--accent);
    color: #fff; font-weight: 600; cursor: pointer;
  }
  .kind-picker {
    display: flex; align-items: center; gap: 0.75rem; flex-wrap: wrap;
    background: var(--surface); border-radius: 10px;
    padding: 0.75rem 1rem; margin-bottom: 1rem;
    border: 1px solid var(--border);
  }
  .kind-label { font-size: 0.85rem; color: var(--muted); white-space: nowrap; }
  .kind-select {
    flex: 1; min-width: 140px;
    padding: 0.5rem 0.75rem; border-radius: 7px;
    border: 1px solid var(--border); background: var(--input-bg);
    color: var(--text); font-size: 0.95rem; cursor: pointer;
  }
  .kind-actions { display: flex; gap: 0.5rem; margin-left: auto; }
  .btn-create {
    padding: 0.5rem 1.25rem; border-radius: 7px;
    border: none; background: var(--accent);
    color: #fff; font-weight: 600; cursor: pointer; font-size: 0.9rem;
  }
  .btn-cancel {
    padding: 0.5rem 1rem; border-radius: 7px;
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-size: 0.9rem;
  }
  .btn-cancel:hover { color: var(--text); border-color: var(--text); }
  .note-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.5rem; }
  li { display: flex; align-items: center; }
  .note-card {
    flex: 1; display: flex; align-items: center; gap: 0.75rem;
    padding: 0.75rem 1rem; border-radius: 8px;
    border: 1px solid var(--border); background: var(--surface);
    text-decoration: none; color: var(--text);
  }
  .note-card:hover { background: var(--hover); }
  .kind-badge { font-size: 1.3rem; }
  .note-info { flex: 1; }
  .lock { font-size: 0.75rem; margin-left: 4px; }
  .tags { display: flex; gap: 4px; margin-top: 4px; flex-wrap: wrap; }
  .tag {
    font-size: 0.7rem; padding: 1px 6px;
    background: var(--accent-muted); border-radius: 12px; color: var(--accent);
  }
  .date { font-size: 0.8rem; color: var(--muted); white-space: nowrap; }
  .del { background: none; border: none; cursor: pointer; margin-left: 0.5rem; font-size: 1rem; opacity: 0.5; }
  .del:hover { opacity: 1; }
  .empty { color: var(--muted); padding: 2rem; text-align: center; }
</style>
