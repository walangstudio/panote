<script lang="ts">
  import {
    transferOfferRespond, noteReceiveAccept, noteReceiveReject,
    type PendingOffer, type PendingTransfer,
  } from "$lib/tauri";
  import { receiveTarget } from "$lib/stores/receiveTarget";

  interface Props {
    offers: PendingOffer[];
    /// Single notes already delivered and held encrypted until the recipient
    /// enters the code. A different protocol from an offer, same thing to ask.
    transfers?: PendingTransfer[];
    onupdate: () => void;
  }
  let { offers, transfers = [], onupdate }: Props = $props();

  let codes = $state<Record<string, string>>({});
  let busy = $state<Record<string, boolean>>({});
  let errors = $state<Record<string, string>>({});
  let dismissed = $state<Set<string>>(new Set());

  /// The two arrivals differ only in how they are unlocked, so they are
  /// normalised to one list and rendered by the same toast.
  interface Incoming {
    id: string;
    from: string;
    what: string;
    unlock: (code: string) => Promise<unknown>;
    drop: () => void;
  }

  let items = $derived<Incoming[]>([
    ...offers.map((o) => ({
      id: o.offer_id,
      from: o.from_peer,
      what: `${o.note_count} ${o.note_count === 1 ? "note" : "notes"}`,
      unlock: (code: string) => transferOfferRespond(o.offer_id, code, $receiveTarget?.id ?? null),
      // An offer is a live connection waiting on us; letting it time out is the
      // only way to decline.
      drop: () => {},
    })),
    ...transfers.map((t) => ({
      id: t.transfer_id,
      from: t.from_peer,
      what: "1 note",
      unlock: (code: string) => noteReceiveAccept(t.transfer_id, code, $receiveTarget?.id ?? null),
      // Already delivered and sitting in memory, so declining must discard it.
      drop: () => { void noteReceiveReject(t.transfer_id).catch(() => {}); },
    })),
  ].filter((i) => !dismissed.has(i.id)));

  async function accept(o: Incoming) {
    const code = (codes[o.id] ?? "").replace(/-/g, "").toUpperCase();
    if (!code) { errors = { ...errors, [o.id]: "Enter the code from the sender." }; return; }
    busy = { ...busy, [o.id]: true };
    errors = { ...errors, [o.id]: "" };
    try {
      await o.unlock(code);
      onupdate();
    } catch (e) {
      errors = { ...errors, [o.id]: String(e) };
    }
    busy = { ...busy, [o.id]: false };
  }

  function dismiss(o: Incoming) {
    o.drop();
    dismissed = new Set(dismissed).add(o.id);
    onupdate();
  }
</script>

{#if items.length > 0}
  <div class="toast-stack">
    {#each items as o (o.id)}
      <div class="toast">
        <span class="badge">
          <span class="material-symbols-outlined">download</span>
        </span>
        <div class="text-block">
          <div class="line1">{o.from} wants to send</div>
          <div class="line2">{o.what}{#if $receiveTarget?.name} into {$receiveTarget.name}{/if}</div>
          <input
            class="code-input"
            placeholder="Enter code (e.g. K4X-7P2)"
            bind:value={codes[o.id]}
            onkeydown={(e) => { if (e.key === "Enter") accept(o); }}
          />
          {#if errors[o.id]}
            <span class="err">{errors[o.id]}</span>
          {/if}
        </div>
        <div class="actions">
          <button class="btn-decline" onclick={() => dismiss(o)} disabled={busy[o.id]}>Dismiss</button>
          <button class="btn-accept" onclick={() => accept(o)} disabled={busy[o.id]}>
            {busy[o.id] ? "Accepting…" : "Accept"}
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
