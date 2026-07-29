<script lang="ts">
  import { onMount, onDestroy } from "svelte";

  interface Props {
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    /// Optional middle action, e.g. "Discard" next to a primary "Save".
    /// Rendered only when both `altLabel` and `onalt` are given.
    altLabel?: string;
    onalt?: () => void;
    destructive?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  }
  let {
    title,
    message,
    confirmLabel = "Confirm",
    cancelLabel = "Cancel",
    altLabel,
    onalt,
    destructive = false,
    onconfirm,
    oncancel,
  }: Props = $props();

  let cancelBtn: HTMLButtonElement | undefined = $state();
  let confirmBtn: HTMLButtonElement | undefined = $state();
  let previouslyFocused: HTMLElement | null = null;

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); oncancel(); }
    // Enter is deliberately NOT handled here. A window-level Enter handler fired
    // `onconfirm` regardless of what had focus, so a stray Enter permanently
    // deleted a note — and made "Discard" unreachable by keyboard in the
    // three-action dialog, because Enter always ran Save instead. Native button
    // activation already does the right thing for whichever button has focus.
  }

  onMount(() => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    window.addEventListener("keydown", onKey);
    // Focus the SAFE action: a destructive dialog defaults to Cancel, so the
    // reflexive Enter is harmless. Anything else defaults to its primary.
    (destructive ? cancelBtn : confirmBtn)?.focus();
  });
  onDestroy(() => {
    window.removeEventListener("keydown", onKey);
    previouslyFocused?.focus?.();
  });
</script>

<div class="overlay">
  <div class="backdrop" role="presentation" onclick={oncancel}></div>
  <div class="modal" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
    <h2 id="confirm-title">{title}</h2>
    <p class="message">{message}</p>
    <div class="actions">
      <button class="btn-cancel" bind:this={cancelBtn} onclick={oncancel}>{cancelLabel}</button>
      {#if altLabel && onalt}
        <button class="btn-alt" onclick={onalt}>{altLabel}</button>
      {/if}
      <button class="btn-confirm" class:destructive bind:this={confirmBtn} onclick={onconfirm}>{confirmLabel}</button>
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
    width: min(400px, 92%);
    box-shadow: 0 16px 48px var(--shadow-color-hover);
    animation: panote-pop-in 0.18s ease;
  }
  h2 {
    margin: 0 0 0.8rem;
    font-size: 1.1rem; font-weight: 700;
    color: var(--text);
  }
  .message {
    margin: 0 0 1.4rem;
    font-size: 0.9rem;
    color: var(--text-secondary);
    line-height: 1.5;
  }
  .actions {
    display: flex; gap: 0.6rem; justify-content: flex-end;
  }
  .btn-cancel {
    padding: 0.55rem 1rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-weight: 600; font-family: inherit;
    transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--accent); color: var(--accent); }
  .btn-alt {
    padding: 0.55rem 1rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--error); cursor: pointer; font-weight: 600; font-family: inherit;
    transition: all 0.15s ease;
  }
  .btn-alt:hover { border-color: var(--error); background: var(--error-surface); }
  .btn-confirm {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; cursor: pointer; font-family: inherit;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease;
  }
  .btn-confirm:hover { transform: scale(1.03); }
  .btn-confirm.destructive { background: var(--error); }
</style>
