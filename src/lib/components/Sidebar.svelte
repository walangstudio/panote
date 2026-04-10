<script lang="ts">
  import { page } from "$app/state";
  import { toggleDarkMode, theme } from "$lib/stores/theme";
  import { sidebarOpen } from "$lib/stores/sidebar";

  interface Props {
    receiving: boolean;
    ontogglereceive: () => void;
    onnewnote: () => void;
  }
  let { receiving, ontogglereceive, onnewnote }: Props = $props();

  const activeTab = $derived(page.url.pathname.startsWith("/settings") ? "settings" : "notes");

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
<aside class="drawer" class:open={$sidebarOpen}>
  <div class="drawer-header">
    <span class="logo-text">Panote</span>
    <button class="close-btn" onclick={() => sidebarOpen.set(false)} aria-label="Close menu">
      <span class="material-symbols-outlined">close</span>
    </button>
  </div>

  <button class="new-note-btn" onclick={newNote}>
    <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' 1;">add</span>
    New Note
  </button>

  <nav>
    <a href="/" class="nav-item" class:active={activeTab === "notes"} onclick={nav}>
      <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' {activeTab === 'notes' ? 1 : 0};">description</span>
      <span>Notes</span>
    </a>
    <a href="/settings" class="nav-item" class:active={activeTab === "settings"} onclick={nav}>
      <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' {activeTab === 'settings' ? 1 : 0};">settings</span>
      <span>Settings</span>
    </a>
  </nav>

  <div class="drawer-bottom">
    <button class="receive-row" onclick={ontogglereceive}>
      <span class="material-symbols-outlined" style="font-size: 20px;">download</span>
      <span class="receive-label">Receiving</span>
      <span class="toggle-pill" class:active={receiving}>
        <span class="toggle-knob"></span>
      </span>
    </button>

    <button class="theme-toggle" onclick={toggleDarkMode}>
      <span class="material-symbols-outlined">{$theme === "candy-dark" ? "light_mode" : "dark_mode"}</span>
      <span>{$theme === "candy-dark" ? "Light mode" : "Dark mode"}</span>
    </button>
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
    font-size: 1.5rem; font-weight: 900; color: var(--accent);
    letter-spacing: -0.02em;
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

  nav { display: flex; flex-direction: column; gap: 0.25rem; flex: 1; }
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

  .theme-toggle {
    display: flex; align-items: center; gap: 0.6rem;
    background: none; border: 1px solid var(--border); border-radius: var(--radius-full);
    padding: 0.5rem 0.75rem; cursor: pointer; color: var(--muted);
    font-size: 0.82rem; font-weight: 500; width: 100%;
    transition: all 0.15s ease;
  }
  .theme-toggle:hover { border-color: var(--accent); color: var(--accent); }
</style>
