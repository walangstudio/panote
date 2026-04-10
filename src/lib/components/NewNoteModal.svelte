<script lang="ts">
  import { goto } from "$app/navigation";

  interface Props { onclose: () => void; }
  let { onclose }: Props = $props();

  const kinds = [
    { id: "document", icon: "edit_note", label: "Document", color: "accent", desc: "Text, markdown, or code" },
    { id: "checklist", icon: "checklist", label: "Checklist", color: "tertiary", desc: "To-do items" },
    { id: "kanban", icon: "view_kanban", label: "Kanban", color: "tertiary", desc: "Board view" },
    { id: "table", icon: "table_chart", label: "Table", color: "secondary", desc: "Parsed data table" },
  ] as const;

  function pick(id: string) {
    onclose();
    goto(`/note/new?kind=${id}`);
  }
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true">
  <button class="close" onclick={onclose} aria-label="Close">
    <span class="material-symbols-outlined">close</span>
  </button>
  <h2>New Note</h2>
  <p class="subtitle">Choose a note type</p>
  <div class="grid">
    {#each kinds as k}
      <button class="kind-card" onclick={() => pick(k.id)}>
        <span class="kind-icon {k.color}">
          <span class="material-symbols-outlined">{k.icon}</span>
        </span>
        <span class="kind-label">{k.label}</span>
        <span class="kind-desc">{k.desc}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0, 0, 0, 0.45); backdrop-filter: blur(4px);
  }
  .modal {
    position: fixed; z-index: 101;
    top: 50%; left: 50%; transform: translate(-50%, -50%);
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg); padding: 1.75rem;
    width: min(400px, 92vw);
    box-shadow: 0 16px 48px var(--shadow-color-hover);
  }
  .close {
    position: absolute; top: 0.75rem; right: 0.75rem;
    background: var(--accent-muted); border: none; border-radius: var(--radius-full);
    color: var(--muted); cursor: pointer;
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .close:hover { background: var(--accent); color: var(--on-accent); }
  h2 { margin: 0 0 0.25rem; font-size: 1.1rem; font-weight: 700; }
  .subtitle { color: var(--muted); font-size: 0.85rem; margin: 0 0 1rem; }
  .grid {
    display: grid; grid-template-columns: repeat(2, 1fr); gap: 0.75rem;
  }
  .kind-card {
    display: flex; flex-direction: column; align-items: center; gap: 0.4rem;
    padding: 1rem 0.5rem; border-radius: var(--radius);
    border: 1px solid var(--border); background: var(--surface);
    cursor: pointer; transition: all 0.2s ease;
  }
  .kind-card:hover {
    background: var(--hover);
    box-shadow: 0 4px 16px var(--shadow-color-hover);
    transform: translateY(-2px);
    border-color: var(--accent-muted);
  }
  .kind-icon {
    width: 44px; height: 44px; border-radius: 12px;
    display: flex; align-items: center; justify-content: center;
  }
  .kind-icon.accent { background: var(--accent-surface); color: var(--accent); }
  .kind-icon.secondary { background: var(--secondary-surface); color: var(--secondary); }
  .kind-icon.tertiary { background: var(--tertiary-surface); color: var(--tertiary); }
  .kind-label { font-weight: 700; font-size: 0.9rem; color: var(--text); }
  .kind-desc { font-size: 0.72rem; color: var(--muted); }
  /* Last item (kanban) spans full width when odd count */
  .kind-card:last-child:nth-child(odd) { grid-column: 1 / -1; }
</style>
