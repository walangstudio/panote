<script lang="ts">
  import { transferOfferRespond, type PendingOffer } from "$lib/tauri";

  interface Props {
    offers: PendingOffer[];
    onupdate: () => void;
  }
  let { offers, onupdate }: Props = $props();

  let codes = $state<Record<string, string>>({});
  let busy = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string>>({});
  let dismissed = $state<Set<string>>(new Set());

  let visibleOffers = $derived(offers.filter((o) => !dismissed.has(o.offer_id)));

  async function accept(o: PendingOffer) {
    const code = (codes[o.offer_id] ?? "").replace(/-/g, "").toUpperCase();
    if (!code) { errors = { ...errors, [o.offer_id]: "Enter the code from the sender." }; return; }
    busy = { ...busy, [o.offer_id]: true };
    errors = { ...errors, [o.offer_id]: "" };
    try {
      await transferOfferRespond(o.offer_id, code);
      onupdate();
    } catch (e) {
      errors = { ...errors, [o.offer_id]: String(e) };
    }
    busy = { ...busy, [o.offer_id]: false };
  }

  function dismiss(o: PendingOffer) {
    // Hide for this session — the sender will time out
    dismissed = new Set(dismissed).add(o.offer_id);
  }
</script>

{#if visibleOffers.length > 0}
  <div class="toast-stack">
    {#each visibleOffers as o (o.offer_id)}
      <div class="toast">
        <span class="badge">
          <span class="material-symbols-outlined">download</span>
        </span>
        <div class="text-block">
          <div class="line1">{o.from_peer} wants to send</div>
          <div class="line2">{o.note_count} {o.note_count === 1 ? "note" : "notes"}</div>
          <input
            class="code-input"
            placeholder="Enter code (e.g. K4X-7P2)"
            bind:value={codes[o.offer_id]}
            onkeydown={(e) => { if (e.key === "Enter") accept(o); }}
          />
          {#if errors[o.offer_id]}
            <span class="err">{errors[o.offer_id]}</span>
          {/if}
        </div>
        <div class="actions">
          <button class="btn-decline" onclick={() => dismiss(o)} disabled={busy[o.offer_id]}>Dismiss</button>
          <button class="btn-accept" onclick={() => accept(o)} disabled={busy[o.offer_id]}>
            {busy[o.offer_id] ? "Accepting…" : "Accept"}
          </button>
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .toast-stack {
    position: fixed;
    top: 12px;
    left: 12px;
    right: 12px;
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .toast {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0.7rem 0.8rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    background: var(--surface-glass);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
    animation: panote-slide-down 0.22s ease;
  }

  .badge {
    width: 38px;
    height: 38px;
    border-radius: var(--radius-full);
    background: var(--tertiary-surface);
    color: var(--tertiary);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    font-size: 20px;
  }

  .text-block {
    flex: 1;
    min-width: 0;
  }

  .line1 {
    font-size: 0.85rem;
    font-weight: 700;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .line2 {
    font-size: 0.78rem;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .code-input {
    width: 100%;
    margin-top: 0.4rem;
    padding: 0.45rem 0.75rem;
    border-radius: var(--radius-full);
    border: 1px solid var(--border);
    background: var(--input-bg);
    color: var(--text);
    font-size: 0.82rem;
    font-family: monospace;
    text-transform: uppercase;
    box-sizing: border-box;
    transition: box-shadow 0.15s ease;
  }

  .code-input:focus {
    box-shadow: 0 0 0 2px var(--accent-muted);
    outline: none;
  }

  .err {
    font-size: 0.75rem;
    color: var(--error);
    display: block;
    margin-top: 0.2rem;
  }

  .actions {
    display: flex;
    gap: 6px;
    flex-shrink: 0;
    align-self: flex-start;
    padding-top: 2px;
  }

  .btn-decline {
    padding: 0.4rem 0.8rem;
    border-radius: var(--radius-full);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--text-secondary);
    font-family: inherit;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    transition: border-color 0.15s ease, color 0.15s ease;
  }

  .btn-decline:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .btn-decline:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-accept {
    padding: 0.4rem 0.8rem;
    border-radius: var(--radius-full);
    border: none;
    background: var(--accent);
    color: var(--on-accent);
    font-family: inherit;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    transition: opacity 0.15s ease;
  }

  .btn-accept:hover {
    opacity: 0.88;
  }

  .btn-accept:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
