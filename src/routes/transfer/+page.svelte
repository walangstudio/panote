<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import {
    peersScan,
    noteSend,
    pendingTransfersList,
    noteReceiveAccept,
    noteReceiveReject,
    type Peer,
    type PendingTransfer,
  } from "$lib/tauri";
  import { notes, refreshNotes } from "$lib/stores/notes";

  let peers = $state<Peer[]>([]);
  let scanning = $state(false);
  let sending = $state(false);
  let status = $state("");

  let selectedNote = $state("");
  let selectedPeer = $state("");
  let sendPassphrase = $state("");

  let incoming = $state<PendingTransfer[]>([]);
  let acceptingId = $state<string | null>(null);
  let passphraseInputs = $state<Record<string, string>>({});
  let pollTimer: ReturnType<typeof setInterval>;

  onMount(() => {
    refreshNotes();
    pollIncoming();
    pollTimer = setInterval(pollIncoming, 3000);
  });

  onDestroy(() => clearInterval(pollTimer));

  async function pollIncoming() {
    try {
      incoming = await pendingTransfersList();
      // Initialize passphrase inputs for new transfers
      for (const t of incoming) {
        if (!(t.transfer_id in passphraseInputs)) {
          passphraseInputs[t.transfer_id] = "";
        }
      }
    } catch {
      // vault may not be ready yet
    }
  }

  async function scan() {
    scanning = true;
    status = "";
    try {
      peers = await peersScan();
      if (peers.length === 0) status = "No peers found on LAN or Bluetooth.";
    } catch (e) {
      status = String(e);
    }
    scanning = false;
  }

  async function send() {
    if (!selectedNote || !selectedPeer || !sendPassphrase) return;
    sending = true;
    status = "";
    try {
      await noteSend(selectedNote, selectedPeer, sendPassphrase);
      status = "Note sent. The recipient should enter the same passphrase to accept.";
      sendPassphrase = "";
    } catch (e) {
      status = String(e);
    }
    sending = false;
  }

  async function accept(transferId: string) {
    const passphrase = passphraseInputs[transferId] ?? "";
    if (!passphrase) return;
    acceptingId = transferId;
    try {
      await noteReceiveAccept(transferId, passphrase);
      await refreshNotes();
      incoming = incoming.filter((t) => t.transfer_id !== transferId);
      delete passphraseInputs[transferId];
      status = "Note imported successfully.";
    } catch (e) {
      status = String(e) === "wrong passphrase" ? "Wrong passphrase — try again." : String(e);
    }
    acceptingId = null;
  }

  async function reject(transferId: string) {
    try {
      await noteReceiveReject(transferId);
      incoming = incoming.filter((t) => t.transfer_id !== transferId);
      delete passphraseInputs[transferId];
    } catch (e) {
      status = String(e);
    }
  }

  function formatTime(ts: number) {
    return new Date(ts * 1000).toLocaleTimeString();
  }

  const viaIcon: Record<string, string> = { lan: "📡", ble: "🔵" };
</script>

<div class="layout">
  <header>
    <a href="/" class="back">← Notes</a>
    <h1>Transfer</h1>
  </header>

  <div class="body">
    {#if incoming.length > 0}
      <section class="incoming-section">
        <h2>Incoming ({incoming.length})</h2>
        <ul class="incoming-list">
          {#each incoming as transfer}
            <li class="incoming-item">
              <div class="incoming-meta">
                <strong>From {transfer.from_peer}</strong>
                <span class="time">received {formatTime(transfer.received_at)}</span>
              </div>
              <div class="incoming-accept">
                <input
                  type="password"
                  placeholder="Enter shared passphrase"
                  bind:value={passphraseInputs[transfer.transfer_id]}
                  onkeydown={(e) => e.key === "Enter" && accept(transfer.transfer_id)}
                />
                <div class="incoming-actions">
                  <button
                    class="accept-btn"
                    onclick={() => accept(transfer.transfer_id)}
                    disabled={acceptingId === transfer.transfer_id || !passphraseInputs[transfer.transfer_id]}
                  >
                    {acceptingId === transfer.transfer_id ? "Importing…" : "Accept"}
                  </button>
                  <button
                    class="reject-btn"
                    onclick={() => reject(transfer.transfer_id)}
                    disabled={acceptingId === transfer.transfer_id}
                  >
                    Reject
                  </button>
                </div>
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <section>
      <h2>1. Select a note to send</h2>
      <select bind:value={selectedNote}>
        <option value="">— choose note —</option>
        {#each $notes as note}
          <option value={note.id}>{note.title} ({note.kind})</option>
        {/each}
      </select>
    </section>

    <section>
      <h2>2. Discover peers</h2>
      <button onclick={scan} disabled={scanning}>
        {scanning ? "Scanning…" : "Scan LAN & Bluetooth"}
      </button>
      {#if peers.length > 0}
        <ul class="peer-list">
          {#each peers as peer}
            <li
              class="peer {selectedPeer === peer.id ? 'selected' : ''}"
              onclick={() => (selectedPeer = peer.id)}
              onkeydown={(e) => e.key === "Enter" && (selectedPeer = peer.id)}
              role="option"
              tabindex="0"
              aria-selected={selectedPeer === peer.id}
            >
              {viaIcon[peer.via] ?? "?"} <strong>{peer.name}</strong>
              <span class="addr">{peer.address}:{peer.port}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h2>3. Set a shared passphrase & send</h2>
      <p class="hint">
        Tell the recipient the passphrase out loud or via another channel. They'll
        enter it on their side to accept the note.
      </p>
      <input
        type="password"
        placeholder="Shared passphrase (e.g. a short phrase or PIN)"
        bind:value={sendPassphrase}
      />
      <button
        onclick={send}
        disabled={!selectedNote || !selectedPeer || !sendPassphrase || sending}
        class="send-btn"
      >
        {sending ? "Sending…" : "Send note"}
      </button>
    </section>

    {#if status}<p class="status">{status}</p>{/if}
  </div>
</div>

<style>
  .layout { display: flex; flex-direction: column; height: 100vh; }
  header {
    display: flex; align-items: center; gap: 1rem;
    padding: 0.75rem 1.5rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .back { text-decoration: none; color: var(--accent); font-size: 0.9rem; }
  h1 { margin: 0; font-size: 1.2rem; }
  .body { padding: 2rem; display: flex; flex-direction: column; gap: 2rem; max-width: 600px; }
  h2 { margin: 0 0 0.75rem; font-size: 1rem; color: var(--muted); }

  /* Incoming */
  .incoming-section { }
  .incoming-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.75rem; }
  .incoming-item {
    padding: 1rem;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: var(--accent-muted);
    display: flex; flex-direction: column; gap: 0.6rem;
  }
  .incoming-meta { display: flex; flex-direction: column; gap: 0.15rem; }
  .time { font-size: 0.8rem; color: var(--muted); }
  .incoming-accept { display: flex; flex-direction: column; gap: 0.5rem; }
  .incoming-actions { display: flex; gap: 0.5rem; }
  .accept-btn {
    padding: 0.4rem 1rem; border-radius: 6px;
    background: var(--accent); color: #fff;
    border: none; cursor: pointer; font-size: 0.9rem; font-weight: 600;
  }
  .reject-btn {
    padding: 0.4rem 0.75rem; border-radius: 6px;
    background: transparent; color: var(--muted);
    border: 1px solid var(--border); cursor: pointer; font-size: 0.9rem;
  }
  .accept-btn:disabled, .reject-btn:disabled { opacity: 0.5; cursor: not-allowed; }

  /* Send flow */
  select, input {
    width: 100%; padding: 0.6rem 0.8rem;
    border: 1px solid var(--border); border-radius: 8px;
    background: var(--input-bg); color: var(--text);
    font-size: 0.95rem; box-sizing: border-box;
  }
  button {
    padding: 0.6rem 1.4rem; border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface); color: var(--text);
    cursor: pointer; font-size: 0.95rem;
  }
  button:disabled { opacity: 0.5; cursor: not-allowed; }
  .send-btn { background: var(--accent); color: #fff; border-color: var(--accent); font-weight: 600; }
  .peer-list { list-style: none; margin: 0.75rem 0 0; padding: 0; display: flex; flex-direction: column; gap: 0.5rem; }
  .peer {
    padding: 0.6rem 1rem; border-radius: 8px;
    border: 1px solid var(--border); background: var(--surface);
    cursor: pointer; display: flex; align-items: center; gap: 0.75rem;
  }
  .peer.selected { border-color: var(--accent); background: var(--accent-muted); }
  .addr { font-size: 0.8rem; color: var(--muted); margin-left: auto; }
  .hint { font-size: 0.85rem; color: var(--muted); margin: 0 0 0.6rem; line-height: 1.5; }
  .status { padding: 0.75rem 1rem; border-radius: 8px; background: var(--surface); border: 1px solid var(--border); }
</style>
