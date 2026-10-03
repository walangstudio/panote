<script lang="ts">
  import { page } from "$app/state";
  import { theme, resolvedTheme } from "$lib/stores/theme";
  import { sidebarOpen } from "$lib/stores/sidebar";
  import { folders, buildTree, refreshFolders, type FolderNode } from "$lib/stores/folders";
  import { listFolder, listTrash, openFolder as showFolder, openTrash as showTrash } from "$lib/stores/listState";
  import { folderCreate, folderDelete, folderRename } from "$lib/tauri";

  interface Props {
    receiving: boolean;
    ontogglereceive: () => void;
    onnewnote: () => void;
  }
  let { receiving, ontogglereceive, onnewnote }: Props = $props();

  const activeTab = $derived(page.url.pathname.startsWith("/settings") ? "settings" : "notes");

  const tree = $derived(buildTree($folders));
  /// Collapsed by id. Collapsed-by-default would hide the structure the tree
  /// exists to show, so folders start open.
  let collapsed = $state<Set<string>>(new Set());
  let busy = $state(false);

  function toggle(id: string) {
    const next = new Set(collapsed);
    if (next.has(id)) next.delete(id); else next.add(id);
    collapsed = next;
  }

  function openFolder(id: string | null) {
    sidebarOpen.set(false);
    showFolder(id, page.url.pathname !== "/");
  }

  function openTrash() {
    sidebarOpen.set(false);
    showTrash(page.url.pathname !== "/");
  }

  async function addFolder(parentId: string | null) {
    const name = prompt(parentId ? "Name for the subfolder" : "Name for the new folder");
    if (!name?.trim() || busy) return;
    busy = true;
    try { await folderCreate(name, parentId); await refreshFolders(); } catch {}
    busy = false;
  }

  async function renameFolder(f: FolderNode) {
    const name = prompt("Rename folder", f.name);
    if (!name?.trim() || name === f.name || busy) return;
    busy = true;
    try { await folderRename(f.id, name); await refreshFolders(); } catch {}
    busy = false;
  }

  async function removeFolder(f: FolderNode) {
    // Say what happens to the notes: this is the question anyone deleting a
    // folder actually has, and the answer is reassuring.
    const sub = f.children.length ? " Subfolders go with it." : "";
    if (!confirm(`Delete "${f.name}"?${sub} Its notes are kept and moved out of the folder.`)) return;
    busy = true;
    try {
      await folderDelete(f.id);
      await refreshFolders();
      if ($listFolder === f.id) listFolder.set(null);
    } catch {}
    busy = false;
  }

  function nav() {
    sidebarOpen.set(false);
  }

  function newNote() {
    sidebarOpen.set(false);
    onnewnote();
  }
</script>

{#if $sidebarOpen}
  <div class="backdrop" role="presentation" onclick={() => sidebarOpen.set(false)}></div>
{/if}
<aside class="drawer" class:open={$sidebarOpen} inert={!$sidebarOpen}>
  <div class="drawer-header">
    <span class="logo-text">panote</span>
    <button class="close-btn" onclick={() => sidebarOpen.set(false)} aria-label="Close menu">
      <span class="material-symbols-outlined">close</span>
    </button>
  </div>

  <button class="new-note-btn" onclick={newNote}>
    <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' 1;">add</span>
    New Note
  </button>

  <nav>
    <a href="/" class="nav-item" class:active={activeTab === "notes"} onclick={() => { listTrash.set(false); nav(); }}>
      <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' {activeTab === 'notes' ? 1 : 0};">sticky_note_2</span>
      <span>Notes</span>
    </a>
    <a href="/settings" class="nav-item" class:active={activeTab === "settings"} onclick={nav}>
      <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' {activeTab === 'settings' ? 1 : 0};">settings</span>
      <span>Settings</span>
    </a>
  </nav>

  <!-- Folders. Recursive snippet with a depth guard, the same shape the
       checklist editor uses for its nested items. -->
  <div class="folders">
    <div class="folders-head">
      <span class="folders-label">Folders</span>
      <button class="folder-add" onclick={() => addFolder(null)} aria-label="New folder" disabled={busy}>
        <span class="material-symbols-outlined" style="font-size: 18px;">create_new_folder</span>
      </button>
    </div>

    <button
      class="folder-row all"
      class:selected={$listFolder === null && !$listTrash}
      onclick={() => openFolder(null)}
    >
      <span class="material-symbols-outlined folder-icon">inbox</span>
      <span class="folder-name">All notes</span>
    </button>

    {#snippet renderFolders(nodes: FolderNode[], depth: number)}
      {#each nodes as f (f.id)}
        <div class="folder-line" style="padding-left: {depth * 0.85}rem">
          {#if f.children.length}
            <button
              class="folder-twisty"
              onclick={() => toggle(f.id)}
              aria-label={collapsed.has(f.id) ? `Expand ${f.name}` : `Collapse ${f.name}`}
              aria-expanded={!collapsed.has(f.id)}
            >
              <span class="material-symbols-outlined" style="font-size: 18px;">
                {collapsed.has(f.id) ? "chevron_right" : "expand_more"}
              </span>
            </button>
          {:else}
            <span class="folder-twisty spacer"></span>
          {/if}
          <button
            class="folder-row"
            class:selected={$listFolder === f.id && !$listTrash}
            onclick={() => openFolder(f.id)}
          >
            <span class="material-symbols-outlined folder-icon">folder</span>
            <span class="folder-name">{f.name}</span>
            {#if f.totalCount}<span class="folder-count">{f.totalCount}</span>{/if}
          </button>
          <span class="folder-actions">
            <button onclick={() => addFolder(f.id)} aria-label={`New folder in ${f.name}`} disabled={busy}>
              <span class="material-symbols-outlined" style="font-size: 16px;">add</span>
            </button>
            <button onclick={() => renameFolder(f)} aria-label={`Rename ${f.name}`} disabled={busy}>
              <span class="material-symbols-outlined" style="font-size: 16px;">edit</span>
            </button>
            <button onclick={() => removeFolder(f)} aria-label={`Delete ${f.name}`} disabled={busy}>
              <span class="material-symbols-outlined" style="font-size: 16px;">delete</span>
            </button>
          </span>
        </div>
        {#if f.children.length && !collapsed.has(f.id) && depth < 20}
          {@render renderFolders(f.children, depth + 1)}
        {/if}
      {/each}
    {/snippet}

    {@render renderFolders(tree, 0)}

    {#if tree.length === 0}
      <p class="folders-empty">No folders yet.</p>
    {/if}

    <button class="folder-row all" class:selected={$listTrash} onclick={openTrash}>
      <span class="material-symbols-outlined folder-icon" aria-hidden="true">delete</span>
      <span class="folder-name">Trash</span>
    </button>
  </div>

  <div class="drawer-bottom">
    <button class="receive-row" onclick={ontogglereceive}>
      <span class="material-symbols-outlined" style="font-size: 20px;">wifi_tethering</span>
      <span class="receive-label">Receiving</span>
      <span class="toggle-pill" class:active={receiving}>
        <span class="toggle-knob"></span>
      </span>
    </button>

    <label class="theme-row">
      <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' 1;">{$resolvedTheme === "candy-dark" ? "dark_mode" : "light_mode"}</span>
      <span class="theme-label">Theme</span>
      <select bind:value={$theme}>
        <option value="candy-light">Light</option>
        <option value="candy-dark">Dark</option>
        <option value="system">System</option>
      </select>
    </label>
  </div>
</aside>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 80;
    background: rgba(0, 0, 0, 0.45); backdrop-filter: blur(4px);
  }

  .drawer {
    position: fixed; top: 0; left: 0; bottom: 0; z-index: 81;
    width: 280px;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border-right: 1px solid var(--border);
    display: flex; flex-direction: column;
    padding: 1.5rem 1rem; gap: 0.5rem;
    transform: translateX(-100%);
    transition: transform 0.25s ease;
    box-shadow: 8px 0 32px var(--shadow-color-hover);
  }
  .drawer.open { transform: translateX(0); }

  .drawer-header {
    display: flex; align-items: center; justify-content: space-between;
    padding: 0 0.5rem; margin-bottom: 0.5rem;
  }
  .logo-text {
    font-size: 1.5rem; font-weight: 900;
    letter-spacing: -0.02em;
    background: linear-gradient(120deg, var(--accent), var(--secondary));
    -webkit-background-clip: text; background-clip: text;
    -webkit-text-fill-color: transparent; color: var(--accent);
  }
  .close-btn {
    background: var(--accent-muted); border: none; border-radius: var(--radius-full);
    color: var(--muted); cursor: pointer;
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .close-btn:hover { background: var(--accent); color: var(--on-accent); }

  .new-note-btn {
    width: 100%; padding: 0.75rem 1rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 700; font-size: 0.95rem; cursor: pointer;
    display: flex; align-items: center; justify-content: center; gap: 0.5rem;
    box-shadow: 0 4px 16px var(--shadow-color-hover);
    transition: transform 0.15s ease, box-shadow 0.15s ease;
    margin-bottom: 0.75rem;
  }
  .new-note-btn:hover { transform: scale(1.02); box-shadow: 0 6px 20px var(--shadow-color-hover); }
  .new-note-btn:active { transform: scale(0.97); }

  /* nav no longer takes the slack; the folder list does, so it can scroll. */
  nav { display: flex; flex-direction: column; gap: 0.25rem; }

  .folders {
    flex: 1; min-height: 0; overflow-y: auto;
    margin-top: 0.75rem; padding-top: 0.5rem;
    border-top: 1px solid var(--border);
  }
  .folders-head {
    display: flex; align-items: center; justify-content: space-between;
    padding: 0 0.5rem 0.25rem 1rem;
  }
  .folders-label {
    font-size: 0.7rem; font-weight: 700; letter-spacing: 0.04em;
    text-transform: uppercase; color: var(--muted);
  }
  .folder-add, .folder-twisty, .folder-actions button {
    background: none; border: none; cursor: pointer; color: var(--muted);
    display: flex; align-items: center; justify-content: center; padding: 0;
    border-radius: var(--radius-full);
  }
  .folder-add:hover, .folder-twisty:hover, .folder-actions button:hover { color: var(--accent); }
  /* 44px touch targets, per the same rule the rest of the app follows. */
  .folder-add { width: 44px; height: 44px; }
  .folder-twisty { width: 28px; height: 44px; flex-shrink: 0; }
  .folder-twisty.spacer { pointer-events: none; }

  .folder-line { display: flex; align-items: center; min-width: 0; }
  .folder-row {
    flex: 1; min-width: 0;
    display: flex; align-items: center; gap: 0.6rem;
    padding: 0.55rem 0.6rem; border: none; background: none; cursor: pointer;
    border-radius: var(--radius-full); color: var(--text-secondary);
    font-size: 0.88rem; font-weight: 500; text-align: left; min-height: 44px;
  }
  .folder-row.all { margin-left: 28px; }
  .folder-row:hover { background: var(--hover); color: var(--text); }
  .folder-row.selected { background: var(--accent-muted); color: var(--accent); font-weight: 700; }
  .folder-icon { font-size: 19px; flex-shrink: 0; }
  .folder-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .folder-count {
    margin-left: auto; font-size: 0.7rem; font-weight: 600; color: var(--muted);
    background: var(--surface-container); padding: 1px 8px; border-radius: var(--radius-full);
  }
  /* Revealed on hover or keyboard focus, so the row stays readable but the
     actions are still reachable without a mouse. */
  .folder-actions { display: flex; opacity: 0; flex-shrink: 0; }
  .folder-actions button { width: 30px; height: 44px; }
  .folder-line:hover .folder-actions,
  .folder-actions:focus-within { opacity: 1; }
  .folders-empty { margin: 0.5rem 1rem; font-size: 0.8rem; color: var(--muted); }
  .nav-item {
    padding: 0.6rem 1rem; border-radius: var(--radius-full);
    text-decoration: none; color: var(--text-secondary); font-size: 0.9rem; font-weight: 500;
    display: flex; align-items: center; gap: 0.75rem;
    transition: all 0.15s ease;
  }
  .nav-item:hover { background: var(--hover); color: var(--text); }
  .nav-item.active {
    background: var(--accent); color: var(--on-accent);
    box-shadow: 0 4px 12px var(--shadow-color-hover);
  }

  .drawer-bottom { margin-top: auto; display: flex; flex-direction: column; gap: 0.5rem; }

  .receive-row {
    display: flex; align-items: center; gap: 0.6rem;
    background: none; border: 1px solid var(--border); border-radius: var(--radius-full);
    padding: 0.55rem 0.75rem; cursor: pointer; color: var(--text-secondary);
    font-size: 0.85rem; font-weight: 500; width: 100%;
    transition: all 0.15s ease;
  }
  .receive-row:hover { border-color: var(--accent); color: var(--accent); }
  .receive-label { flex: 1; text-align: left; }

  .toggle-pill {
    width: 40px; height: 22px; border-radius: 11px;
    background: var(--surface-container); border: 1px solid var(--border);
    position: relative; flex-shrink: 0;
    transition: all 0.2s ease;
  }
  .toggle-pill.active { background: var(--accent); border-color: var(--accent); }
  .toggle-knob {
    position: absolute; top: 2px; left: 2px;
    width: 16px; height: 16px; border-radius: 50%;
    background: var(--muted);
    transition: all 0.2s ease;
  }
  .toggle-pill.active .toggle-knob {
    left: 20px; background: var(--on-accent);
  }

  .theme-row {
    display: flex; align-items: center; gap: 0.6rem;
    background: none; border: 1px solid var(--border); border-radius: var(--radius-full);
    padding: 0.5rem 0.75rem; cursor: pointer; color: var(--muted);
    font-size: 0.82rem; font-weight: 500; width: 100%;
    transition: all 0.15s ease;
  }
  .theme-row:hover, .theme-row:focus-within { border-color: var(--accent); color: var(--accent); }
  .theme-label { flex: 1; text-align: left; }
  .theme-row select {
    background: none; border: none; padding: 0; outline: none;
    color: inherit; font: inherit; cursor: pointer;
  }
</style>
