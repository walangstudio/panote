<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { folders, buildTree, type FolderNode } from "$lib/stores/folders";
  import { trapFocus } from "$lib/trapFocus";

  interface Props {
    title?: string;
    /// Currently selected destination, so it can be shown as the current one.
    current?: string | null;
    /// When moving a folder, that folder: it and its descendants are not valid
    /// destinations. The backend refuses them too, but offering a choice that
    /// will always fail is a worse experience than not offering it.
    excludeSubtreeOf?: string | null;
    onpick: (folderId: string | null) => void;
    onclose: () => void;
  }
  let { title = "Move to", current = null, excludeSubtreeOf = null, onpick, onclose }: Props = $props();

  const tree = $derived(buildTree($folders));

  /// The moving folder plus everything under it. Walked from the flat list with a
  /// depth cap so a malformed row cannot loop.
  const forbidden = $derived(() => {
    const out = new Set<string>();
    if (!excludeSubtreeOf) return out;
    out.add(excludeSubtreeOf);
    const walk = (parent: string, depth: number) => {
      if (depth > 20) return;
      for (const f of $folders) {
        if (f.parent_id === parent) { out.add(f.id); walk(f.id, depth + 1); }
      }
    };
    walk(excludeSubtreeOf, 0);
    return out;
  });

  let closeBtn: HTMLButtonElement | undefined = $state();
  let previouslyFocused: HTMLElement | null = null;

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onclose(); }
  }

  onMount(() => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    closeBtn?.focus();
    window.addEventListener("keydown", onKey);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    previouslyFocused?.focus?.();
  });
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true" aria-label={title} use:trapFocus>
  <button class="close" bind:this={closeBtn} onclick={onclose} aria-label="Close">
    <span class="material-symbols-outlined" aria-hidden="true">close</span>
  </button>
  <h2>{title}</h2>

  <div class="dest-list">
    <button
      class="dest"
      class:selected={current === null}
      onclick={() => onpick(null)}
    >
      <span class="material-symbols-outlined dest-ico" aria-hidden="true">inbox</span>
      <span class="dest-name">Home</span>
      {#if current === null}<span class="here">Current</span>{/if}
    </button>

    {#snippet renderDests(nodes: FolderNode[], depth: number)}
      {#each nodes as f (f.id)}
        {@const blocked = forbidden().has(f.id)}
        <button
          class="dest"
          class:selected={current === f.id}
          style="padding-left: {0.75 + depth * 1}rem"
          disabled={blocked}
          title={blocked ? "A folder cannot be moved inside itself" : undefined}
          onclick={() => onpick(f.id)}
        >
          <span class="material-symbols-outlined dest-ico" aria-hidden="true">folder</span>
          <span class="dest-name">{f.name}</span>
          {#if current === f.id}<span class="here">Current</span>{/if}
        </button>
        {#if f.children.length && depth < 20}
          {@render renderDests(f.children, depth + 1)}
        {/if}
      {/each}
    {/snippet}

    {@render renderDests(tree, 0)}
  </div>

  {#if tree.length === 0}
    <p class="hint">No folders yet. Create one first.</p>
  {/if}
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; background: var(--backdrop);
    backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px); z-index: 300;
  }
  .modal {
    position: fixed; z-index: 301;
    top: 50%; left: 50%; transform: translate(-50%, -50%);
    width: min(420px, calc(100vw - 2rem));
    max-height: min(70vh, 560px);
    display: flex; flex-direction: column;
    background: var(--surface); border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    box-shadow: 0 20px 60px var(--shadow-color-hover);
    padding: 1.25rem;
  }
  .close {
    position: absolute; top: 0.6rem; right: 0.6rem;
    width: 40px; height: 40px; border: none; background: none;
    color: var(--muted); cursor: pointer; border-radius: var(--radius-full);
    display: flex; align-items: center; justify-content: center;
  }
  .close:hover { background: var(--hover); color: var(--text); }
  h2 { margin: 0 2.5rem 0.85rem 0; font-size: 1.05rem; }
  .dest-list { overflow-y: auto; min-height: 0; display: flex; flex-direction: column; gap: 2px; }
  .dest {
    display: flex; align-items: center; gap: 0.6rem;
    padding: 0.6rem 0.75rem; min-height: 44px;
    border: none; background: none; cursor: pointer; text-align: left;
    border-radius: var(--radius); color: var(--text); font-size: 0.9rem;
  }
  .dest:hover:not(:disabled) { background: var(--hover); }
  .dest.selected { background: var(--accent-muted); color: var(--accent); font-weight: 700; }
  .dest:disabled { opacity: 0.4; cursor: not-allowed; }
  .dest-ico { font-size: 20px; flex-shrink: 0; }
  .dest-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .here {
    margin-left: auto; font-size: 0.68rem; font-weight: 600; color: var(--muted);
    background: var(--surface-container); padding: 1px 8px; border-radius: var(--radius-full);
  }
  .hint { margin: 0.75rem 0 0; font-size: 0.82rem; color: var(--muted); }
</style>
