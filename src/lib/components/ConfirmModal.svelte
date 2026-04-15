<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  interface Props {
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    destructive?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  }
  let {
    title,
    message,
    confirmLabel = "Confirm",
    cancelLabel = "Cancel",
    destructive = false,
    onconfirm,
    oncancel,
  }: Props = $props();

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); oncancel(); }
    else if (e.key === "Enter") { e.preventDefault(); onconfirm(); }
  }

  onMount(() => { window.addEventListener("keydown", onKey); });
  onDestroy(() => { window.removeEventListener("keydown", onKey); });
</script>

<div class="backdrop" role="presentation" onclick={oncancel}></div>
<div class="modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
  <h2 id="confirm-title">{title}</h2>
  <p class="message">{message}</p>
  <div class="actions">
    <button class="btn-cancel" onclick={oncancel}>{cancelLabel}</button>
    <button class="btn-confirm" class:destructive onclick={onconfirm}>{confirmLabel}</button>
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
    background: var(--surface-glass);
    backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg); padding: 1.5rem 1.75rem;
    width: min(400px, 92vw);
    box-shadow: 0 16px 48px var(--shadow-color-hover);
  }
  h2 {
    margin: 0 0 0.5rem;
    font-size: 1.1rem; font-weight: 700;
    color: var(--text);
  }
  .message {
    margin: 0 0 1.25rem;
    font-size: 0.9rem;
    color: var(--text-secondary);
    line-height: 1.45;
  }
  .actions {
    display: flex; gap: 0.6rem; justify-content: flex-end;
  }
  .btn-cancel {
    padding: 0.55rem 1rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-weight: 600;
    transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--accent); color: var(--accent); }
  .btn-confirm {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; cursor: pointer;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease;
  }
  .btn-confirm:hover { transform: scale(1.03); }
  .btn-confirm.destructive { background: var(--error); }
</style>
