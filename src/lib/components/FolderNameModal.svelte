<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { trapFocus } from "$lib/trapFocus";

  interface Props {
    title?: string;
    confirmLabel?: string;
    initial?: string;
    /// Surfaced from the backend, which is what actually enforces the tree rules.
    error?: string;
    onsubmit: (name: string) => void;
    onclose: () => void;
  }
  let {
    title = "New folder",
    confirmLabel = "Create",
    initial = "",
    error = "",
    onsubmit,
    onclose,
  }: Props = $props();

  let name = $state(initial);
  let input: HTMLInputElement | undefined = $state();
  let previouslyFocused: HTMLElement | null = null;

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onclose(); }
  }

  onMount(() => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    input?.focus();
    input?.select();
    window.addEventListener("keydown", onKey);
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    previouslyFocused?.focus?.();
  });

  function submit(e: Event) {
    e.preventDefault();
    if (name.trim()) onsubmit(name.trim());
  }
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true" aria-label={title} use:trapFocus>
  <h2>{title}</h2>
  <form onsubmit={submit}>
    <input
      class="name-field"
      bind:this={input}
      bind:value={name}
      placeholder="Folder name"
      maxlength="200"
      aria-label="Folder name"
    />
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="actions">
      <button type="button" class="btn-cancel" onclick={onclose}>Cancel</button>
      <button type="submit" class="btn-confirm" disabled={!name.trim()}>{confirmLabel}</button>
    </div>
  </form>
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; background: var(--backdrop);
    backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px); z-index: 300;
  }
  .modal {
    position: fixed; z-index: 301;
    top: 50%; left: 50%; transform: translate(-50%, -50%);
    width: min(400px, calc(100vw - 2rem));
    background: var(--surface); border-radius: var(--radius-lg);
    border: 1px solid var(--border);
    box-shadow: 0 20px 60px var(--shadow-color-hover);
    padding: 1.25rem;
  }
  h2 { margin: 0 0 0.85rem; font-size: 1.05rem; }
  .name-field {
    width: 100%; box-sizing: border-box;
    padding: 0.7rem 0.85rem; min-height: 44px;
    border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface-container); color: var(--text); font-size: 0.95rem;
  }
  .name-field:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
  .error { margin: 0.6rem 0 0; font-size: 0.82rem; color: var(--error); }
  .actions { display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 1rem; }
  .btn-cancel, .btn-confirm {
    padding: 0.6rem 1.1rem; min-height: 44px;
    border-radius: var(--radius-full); cursor: pointer; font-weight: 600; font-size: 0.9rem;
  }
  .btn-cancel { border: 1px solid var(--border); background: none; color: var(--text-secondary); }
  .btn-cancel:hover { background: var(--hover); }
  .btn-confirm { border: none; background: var(--accent); color: var(--on-accent); }
  .btn-confirm:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
