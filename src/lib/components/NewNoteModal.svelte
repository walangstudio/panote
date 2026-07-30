<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { goto } from "$app/navigation";
  import { trapFocus } from "$lib/trapFocus";

  interface Props {
    onclose: () => void;
    /// A folder is created where the list already is, so the caller owns it.
    onnewfolder?: () => void;
  }
  let { onclose, onnewfolder }: Props = $props();

  let previouslyFocused: HTMLElement | null = null;
  let kindButtons: (HTMLButtonElement | undefined)[] = $state([]);

  const kinds = [
    { id: "document", icon: "edit_note", label: "Document", color: "accent", desc: "Markdown, plain or code" },
    { id: "checklist", icon: "checklist", label: "Checklist", color: "tertiary", desc: "Tick off tasks" },
    { id: "kanban", icon: "view_kanban", label: "Kanban", color: "tertiary", desc: "Columns of cards" },
    { id: "table", icon: "table_chart", label: "Table", color: "secondary", desc: "Rows and columns" },
    // A folder is not a note kind, so `pick` routes it separately.
    { id: "folder", icon: "create_new_folder", label: "Folder", color: "secondary", desc: "Group notes together" },
  ] as const;

  function pick(id: string) {
    onclose();
    // A folder is created where you are, not by navigating to an editor.
    if (id === "folder") { onnewfolder?.(); return; }
    goto(`/note/new?kind=${id}`);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onclose(); }
  }
  onMount(() => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    window.addEventListener("keydown", onKey);
    kindButtons[0]?.focus();
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    previouslyFocused?.focus?.();
  });
</script>

<div class="overlay">
  <div class="backdrop" role="presentation" onclick={onclose}></div>
  <div class="modal" role="dialog" aria-modal="true" aria-label="New note" use:trapFocus>
    <h2>New note</h2>
    <div class="list">
      {#each kinds as k, i}
        <button class="kind-row" bind:this={kindButtons[i]} onclick={() => pick(k.id)}>
          <span class="kind-icon {k.color}">
            <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' 1;">{k.icon}</span>
          </span>
          <span class="kind-text">
            <span class="kind-label">{k.label}</span>
            <span class="kind-desc">{k.desc}</span>
          </span>
        </button>
      {/each}
    </div>
    <div class="actions">
      <button class="btn-cancel" onclick={onclose}>Cancel</button>
    </div>
  </div>
</div>

<style>
  .overlay {
    position: fixed; inset: 0; z-index: 100;
    display: flex; align-items: center; justify-content: center; padding: 1.1rem;
  }
  .backdrop {
    position: absolute; inset: 0;
    background: rgba(0, 0, 0, 0.45); backdrop-filter: blur(4px); -webkit-backdrop-filter: blur(4px);
    animation: panote-fade-in 0.15s ease;
  }
  .modal {
    position: relative; z-index: 101;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg); padding: 1.5rem 1.6rem;
    width: min(400px, 92vw);
    box-shadow: 0 16px 48px var(--shadow-color-hover);
    animation: panote-pop-in 0.18s ease;
  }
  h2 { margin: 0 0 0.8rem; font-size: 1.1rem; font-weight: 700; }
  .list { display: flex; flex-direction: column; gap: 0.5rem; }
  .kind-row {
    display: flex; align-items: center; gap: 0.85rem; width: 100%; text-align: left;
    padding: 0.7rem 0.8rem; border-radius: var(--radius);
    border: 1px solid var(--border); background: transparent;
    cursor: pointer; transition: border-color 0.15s ease, background 0.15s ease;
  }
  .kind-row:hover { border-color: var(--accent); background: var(--hover); }
  .kind-icon {
    width: 40px; height: 40px; border-radius: 12px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
  }
  .kind-icon.accent { background: var(--accent-surface); color: var(--accent); }
  .kind-icon.secondary { background: var(--secondary-surface); color: var(--secondary); }
  .kind-icon.tertiary { background: var(--tertiary-surface); color: var(--tertiary); }
  .kind-icon .material-symbols-outlined { font-size: 22px; }
  .kind-text { display: flex; flex-direction: column; min-width: 0; }
  .kind-label { font-weight: 700; font-size: 0.95rem; color: var(--text); }
  .kind-desc { font-size: 0.78rem; color: var(--muted); }
  .actions { display: flex; justify-content: flex-end; margin-top: 1rem; }
  .btn-cancel {
    padding: 0.55rem 1rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-weight: 600; font-family: inherit;
    transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--accent); color: var(--accent); }
</style>
