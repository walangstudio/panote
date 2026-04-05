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
    // Just remove from UI — the sender will time out
    onupdate();
  }

  function formatTime(ts: number) {
    return new Date(ts * 1000).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  }
</script>

{#if offers.length > 0}
  <div class="toast-stack">
    {#each offers as o (o.offer_id)}
      <div class="toast">
        <div class="toast-header">
          <span class="from">From: <strong>{o.from_peer}</strong></span>
          <span class="time">{formatTime(o.received_at)}</span>
        </div>
        <p class="offer-info">{o.note_count} {o.note_count === 1 ? "note" : "notes"}</p>
        <div class="toast-body">
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
        <div class="toast-actions">
          <button class="btn-reject" onclick={() => dismiss(o)} disabled={busy[o.offer_id]}>Dismiss</button>
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
    position: fixed; bottom: calc(1.5rem + env(safe-area-inset-bottom, 0px)); left: 50%; transform: translateX(-50%);
    z-index: 200; display: flex; flex-direction: column; gap: 0.75rem;
    width: min(400px, 92vw);
  }
  .toast {
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 12px; padding: 1rem;
    box-shadow: 0 4px 20px rgba(0,0,0,0.18);
  }
  .toast-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.3rem; }
  .from { font-size: 0.9rem; }
  .time { font-size: 0.75rem; color: var(--muted); }
  .offer-info { font-size: 0.82rem; color: var(--muted); margin: 0 0 0.5rem; }
  .toast-body { margin-bottom: 0.6rem; }
  .code-input {
    width: 100%; padding: 0.5rem 0.75rem; border-radius: 8px;
    border: 1px solid var(--border); background: var(--input-bg);
    color: var(--text); font-size: 0.9rem; font-family: monospace;
    text-transform: uppercase; box-sizing: border-box;
  }
  .err { font-size: 0.78rem; color: #e74c3c; display: block; margin-top: 0.25rem; }
  .toast-actions { display: flex; gap: 0.5rem; justify-content: flex-end; }
  .btn-accept {
    padding: 0.45rem 1rem; border-radius: 7px;
    border: none; background: var(--accent); color: #fff;
    font-weight: 600; cursor: pointer; font-size: 0.85rem;
  }
  .btn-accept:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-reject {
    padding: 0.45rem 0.85rem; border-radius: 7px;
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-size: 0.85rem;
  }
  .btn-reject:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
