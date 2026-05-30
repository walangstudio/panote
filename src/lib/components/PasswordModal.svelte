<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { WRONG_PASSWORD } from "$lib/tauri";

  type Mode = "set" | "change" | "unlock" | "remove";

  interface Props {
    mode: Mode;
    title?: string;
    /// Perform the action. Throw to surface an error inline; resolve to close.
    onsubmit: (v: { password: string; oldPassword?: string }) => Promise<void>;
    onclose: () => void;
  }
  let { mode, title, onsubmit, onclose }: Props = $props();

  let current = $state("");
  let next = $state("");
  let confirm = $state("");
  let reveal = $state(false);
  let busy = $state(false);
  let error = $state("");

  const heading = $derived(
    title ??
      ({
        set: "Set password",
        change: "Change password",
        unlock: "Unlock note",
        remove: "Remove password",
      } as const)[mode],
  );
  const submitLabel = $derived(
    ({ set: "Protect", change: "Change", unlock: "Unlock", remove: "Remove" } as const)[mode],
  );
  const needsCurrent = $derived(mode === "change" || mode === "unlock" || mode === "remove");
  const needsNew = $derived(mode === "set" || mode === "change");
  const fieldType = $derived(reveal ? "text" : "password");

  function validate(): string | null {
    if (needsCurrent && !current) return "Enter the current password.";
    if (needsNew) {
      if (!next) return "Enter a password.";
      if (next !== confirm) return "Passwords don't match.";
    }
    return null;
  }

  async function submit() {
    if (busy) return;
    const v = validate();
    if (v) { error = v; return; }
    error = "";
    busy = true;
    try {
      await onsubmit({
        password: needsNew ? next : current,
        oldPassword: needsCurrent ? current : undefined,
      });
      onclose();
    } catch (e) {
      error = String(e) === WRONG_PASSWORD ? "Wrong password." : String(e);
      busy = false;
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onclose(); }
    else if (e.key === "Enter") { e.preventDefault(); submit(); }
  }

  onMount(() => window.addEventListener("keydown", onKey));
  onDestroy(() => window.removeEventListener("keydown", onKey));
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true" aria-labelledby="pw-title">
  <h2 id="pw-title">
    <span class="material-symbols-outlined">
      {mode === "remove" ? "lock_open" : mode === "unlock" ? "lock" : "password"}
    </span>
    {heading}
  </h2>

  {#if mode === "set"}
    <p class="warn">
      <span class="material-symbols-outlined">warning</span>
      There's no recovery. If you forget this password, the note can't be opened — ever.
    </p>
  {/if}

  <div class="fields">
    {#if needsCurrent}
      <input
        class="field"
        type={fieldType}
        placeholder={mode === "change" ? "Current password" : "Password"}
        bind:value={current}
        autocomplete="current-password"
      />
    {/if}
    {#if needsNew}
      <input
        class="field"
        type={fieldType}
        placeholder={mode === "change" ? "New password" : "Password"}
        bind:value={next}
        autocomplete="new-password"
      />
      <input
        class="field"
        type={fieldType}
        placeholder="Confirm password"
        bind:value={confirm}
        autocomplete="new-password"
      />
    {/if}
    <label class="reveal">
      <input type="checkbox" bind:checked={reveal} />
      Show password
    </label>
  </div>

  {#if error}<p class="error">{error}</p>{/if}

  <div class="actions">
    <button class="btn-cancel" onclick={onclose}>Cancel</button>
    <button class="btn-confirm" class:destructive={mode === "remove"} disabled={busy} onclick={submit}>
      {busy ? "…" : submitLabel}
    </button>
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
    margin: 0 0 0.85rem; font-size: 1.1rem; font-weight: 700;
    color: var(--text); display: flex; align-items: center; gap: 0.5rem;
  }
  h2 .material-symbols-outlined { font-size: 20px; color: var(--accent); }
  .warn {
    display: flex; gap: 0.5rem; align-items: flex-start;
    margin: 0 0 1rem; padding: 0.6rem 0.75rem;
    background: var(--accent-muted); border-radius: var(--radius);
    font-size: 0.82rem; color: var(--text-secondary); line-height: 1.4;
  }
  .warn .material-symbols-outlined { font-size: 18px; color: var(--error); flex-shrink: 0; }
  .fields { display: flex; flex-direction: column; gap: 0.6rem; margin-bottom: 0.75rem; }
  .field {
    padding: 0.6rem 0.85rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: var(--input-bg);
    color: var(--text); font-size: 0.9rem;
  }
  .field:focus { outline: none; border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-muted); }
  .reveal {
    display: flex; align-items: center; gap: 0.4rem;
    font-size: 0.8rem; color: var(--muted); cursor: pointer;
  }
  .error { color: var(--error); font-size: 0.82rem; margin: 0 0 0.75rem; }
  .actions { display: flex; gap: 0.6rem; justify-content: flex-end; margin-top: 0.5rem; }
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
  .btn-confirm:hover:not(:disabled) { transform: scale(1.03); }
  .btn-confirm:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-confirm.destructive { background: var(--error); }
</style>
