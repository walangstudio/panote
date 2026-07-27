<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { WRONG_PASSWORD } from "$lib/tauri";

  type Mode = "set" | "change" | "unlock" | "remove" | "recover";

  interface Props {
    mode: Mode;
    title?: string;
    /// Perform the action. Throw to surface an error inline; resolve to close.
    onsubmit: (v: { password: string; oldPassword?: string; recoveryCode?: string }) => Promise<void>;
    onclose: () => void;
  }
  let { mode, title, onsubmit, onclose }: Props = $props();

  let current = $state("");
  let next = $state("");
  let confirm = $state("");
  let code = $state("");
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
        recover: "Recover note",
      } as const)[mode],
  );
  const submitLabel = $derived(
    ({ set: "Encrypt", change: "Change", unlock: "Unlock", remove: "Remove", recover: "Recover" } as const)[mode],
  );
  const needsCurrent = $derived(mode === "change" || mode === "unlock" || mode === "remove");
  const needsNew = $derived(mode === "set" || mode === "change" || mode === "recover");
  const needsCode = $derived(mode === "recover");
  const fieldType = $derived(reveal ? "text" : "password");

  function validate(): string | null {
    if (needsCode && !code.trim()) return "Enter your recovery code.";
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
        recoveryCode: needsCode ? code.trim() : undefined,
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

<div class="overlay">
  <div class="backdrop" role="presentation" onclick={onclose}></div>
  <div class="modal" role="dialog" aria-modal="true" aria-labelledby="pw-title">
    <h2 id="pw-title">
      <span class="material-symbols-outlined">
        {mode === "remove" ? "lock_open" : mode === "unlock" ? "lock" : "password"}
      </span>
      {heading}
    </h2>

    <p class="desc">
      {#if mode === "set"}
        Set a password to encrypt this note. You'll need it to open the note again.
      {:else if mode === "remove"}
        Enter the current password to remove encryption from this note.
      {:else if mode === "change"}
        Enter your current password, then choose a new one.
      {:else if mode === "recover"}
        Enter your recovery code, then set a new password for this note.
      {:else}
        Enter the password to unlock this note.
      {/if}
    </p>

    {#if mode === "set"}
      <div class="warn">
        <span class="material-symbols-outlined">warning</span>
        There's no automatic recovery. Forget the password and the note can't be opened —
        unless you add a recovery code.
      </div>
    {/if}

    <div class="fields">
      {#if needsCode}
        <div class="pill-input">
          <span class="material-symbols-outlined icon">key</span>
          <input
            type="text"
            placeholder="Recovery code"
            bind:value={code}
            autocomplete="off"
            autofocus
          />
        </div>
      {/if}
      {#if needsCurrent}
        <div class="pill-input">
          <span class="material-symbols-outlined icon">password</span>
          <input
            type={fieldType}
            placeholder={mode === "change" ? "Current password" : "Password"}
            bind:value={current}
            autocomplete="current-password"
            autofocus
          />
        </div>
      {/if}
      {#if needsNew}
        <div class="pill-input">
          <span class="material-symbols-outlined icon">password</span>
          <input
            type={fieldType}
            placeholder={mode === "change" ? "New password" : "Password"}
            bind:value={next}
            autocomplete="new-password"
            autofocus={!needsCurrent && !needsCode}
          />
        </div>
        <div class="pill-input">
          <span class="material-symbols-outlined icon">password</span>
          <input
            type={fieldType}
            placeholder="Confirm password"
            bind:value={confirm}
            autocomplete="new-password"
          />
        </div>
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
    margin: 0 0 0.8rem; font-size: 1.1rem; font-weight: 700;
    color: var(--text); display: flex; align-items: center; gap: 0.5rem;
  }
  h2 .material-symbols-outlined { font-size: 20px; color: var(--accent); }
  .desc {
    margin: 0 0 1rem; color: var(--text-secondary); font-size: 0.88rem; line-height: 1.5;
  }
  .warn {
    display: flex; gap: 0.5rem; align-items: flex-start;
    margin: 0 0 1rem; padding: 0.6rem 0.75rem;
    background: var(--accent-muted); border-radius: var(--radius);
    font-size: 0.82rem; color: var(--text-secondary); line-height: 1.4;
  }
  .warn .material-symbols-outlined { font-size: 18px; color: var(--error); flex-shrink: 0; }
  .fields { display: flex; flex-direction: column; gap: 0.6rem; margin-bottom: 0.75rem; }
  .pill-input {
    display: flex; align-items: center; gap: 8px;
    background: var(--input-bg); border: 1px solid var(--border);
    border-radius: var(--radius-full); padding: 0 0.9rem;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
  }
  .pill-input:focus-within {
    border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-muted);
  }
  .pill-input .icon { font-size: 18px; color: var(--muted); flex-shrink: 0; }
  .pill-input input {
    flex: 1; border: none; outline: none; background: transparent;
    padding: 0.7rem 0; font-family: inherit; font-size: 0.95rem; color: var(--text);
  }
  .reveal {
    display: flex; align-items: center; gap: 0.4rem;
    font-size: 0.8rem; color: var(--muted); cursor: pointer;
  }
  .error { color: var(--error); font-size: 0.82rem; margin: 0 0 0.75rem; }
  .actions { display: flex; gap: 0.6rem; justify-content: flex-end; margin-top: 0.5rem; }
  .btn-cancel {
    padding: 0.55rem 1rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-weight: 600; font-family: inherit;
    transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--accent); color: var(--accent); }
  .btn-confirm {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; cursor: pointer; font-family: inherit;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease;
  }
  .btn-confirm:hover:not(:disabled) { transform: scale(1.03); }
  .btn-confirm:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-confirm.destructive { background: var(--error); }
</style>
