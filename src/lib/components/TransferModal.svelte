<script lang="ts">
  import { onMount } from "svelte";
  import {
    peersScan, notesSend, generatePairingCode, knownPeersList, peerAddManual, deviceIps,
    type Peer, type KnownPeer,
  } from "$lib/tauri";

  interface Props {
    noteIds: string[];
    onclose: () => void;
  }
  let { noteIds, onclose }: Props = $props();

  type Step = "peers" | "code" | "sending" | "done" | "error";

  let step = $state<Step>("peers");
  let livePeers = $state<Peer[]>([]);
  let recentPeers = $state<KnownPeer[]>([]);
  let scanning = $state(false);
  let selectedPeer = $state<Peer | null>(null);
  let pairingCode = $state("");
  let errorMsg = $state("");
  let manualIp = $state("");
  let manualBusy = $state(false);
  let manualError = $state("");
  let myIps = $state<string[]>([]);

  onMount(async () => {
    recentPeers = await knownPeersList().catch(() => []);
    myIps = await deviceIps().catch(() => []);
    await scan();
  });

  async function scan() {
    scanning = true;
    livePeers = await peersScan().catch(() => []);
    scanning = false;
  }

  function selectPeer(peer: Peer) {
    selectedPeer = peer;
  }

  // Build a Peer-compatible object from a recent peer (address only, no live port).
  // Only recent peers that are also live can be selected immediately;
  // others are shown greyed out as reference.
  function liveMatchFor(recent: KnownPeer): Peer | null {
    return livePeers.find(p => p.address === recent.peer_id || p.id === recent.peer_id) ?? null;
  }

  async function proceed() {
    if (!selectedPeer) return;
    pairingCode = await generatePairingCode();
    step = "code";
  }

  async function confirmSend() {
    if (!selectedPeer) return;
    step = "sending";
    try {
      await notesSend(noteIds, selectedPeer.id, pairingCode);
      step = "done";
    } catch (e) {
      errorMsg = String(e);
      step = "error";
    }
  }

  async function connectManual() {
    const ip = manualIp.trim();
    if (!ip) return;
    manualBusy = true;
    manualError = "";
    try {
      const peer = await peerAddManual(ip);
      livePeers = [...livePeers.filter(p => p.address !== ip), peer];
      selectedPeer = peer;
      manualIp = "";
    } catch (e) {
      manualError = String(e);
    }
    manualBusy = false;
  }

  function formatDate(ts: number | null) {
    if (!ts) return "";
    return new Date(ts * 1000).toLocaleDateString();
  }
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true">
  <button class="close" onclick={onclose} aria-label="Close">×</button>

  {#if step === "peers"}
    <h2>Send {noteIds.length === 1 ? "note" : `${noteIds.length} notes`}</h2>

    <div class="section-label">Nearby devices</div>
    {#if scanning}
      <p class="muted">Scanning…</p>
    {:else if livePeers.length === 0}
      <p class="muted">No devices found.</p>
    {:else}
      <ul class="peer-list">
        {#each livePeers as peer (peer.id)}
          <li>
            <button
              class="peer-item"
              class:selected={selectedPeer?.id === peer.id}
              onclick={() => selectPeer(peer)}
            >
              <span class="peer-name">{peer.name}</span>
              <span class="peer-via">{peer.via.toUpperCase()}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
    <button class="rescan" onclick={scan} disabled={scanning}>
      {scanning ? "Scanning…" : "Scan again"}
    </button>

    <div class="section-label" style="margin-top: 1rem;">Connect by IP</div>
    {#if myIps.length > 0}
      <p class="my-ips">This device: <strong>{myIps.join(", ")}</strong></p>
    {/if}
    <div class="manual-row">
      <input
        class="manual-input"
        placeholder="e.g. 192.168.1.42"
        bind:value={manualIp}
        onkeydown={(e) => { if (e.key === "Enter") connectManual(); }}
      />
      <button class="btn-connect" onclick={connectManual} disabled={manualBusy || !manualIp.trim()}>
        {manualBusy ? "…" : "Connect"}
      </button>
    </div>
    {#if manualError}<span class="manual-err">{manualError}</span>{/if}

    {#if recentPeers.length > 0}
      <div class="section-label" style="margin-top: 1rem;">Recently contacted</div>
      <ul class="peer-list">
        {#each recentPeers as r (r.peer_id)}
          {@const live = liveMatchFor(r)}
          <li>
            <button
              class="peer-item"
              class:selected={live && selectedPeer?.id === live.id}
              class:dimmed={!live}
              disabled={!live}
              onclick={() => { if (live) selectPeer(live); }}
            >
              <span class="peer-name">{r.display_name ?? r.peer_id}</span>
              <span class="peer-meta">{live ? "online" : `last seen ${formatDate(r.last_transfer_at)}`}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <div class="actions">
      <button class="btn-cancel" onclick={onclose}>Cancel</button>
      <button class="btn-primary" disabled={!selectedPeer} onclick={proceed}>Next</button>
    </div>

  {:else if step === "code"}
    <h2>Share this code</h2>
    <p class="muted">Tell the recipient to enter this code when the transfer arrives.</p>
    <div class="code-display">
      {pairingCode.slice(0, 3)}-{pairingCode.slice(3)}
    </div>
    <p class="muted" style="font-size: 0.8rem;">Sending to: <strong>{selectedPeer?.name}</strong></p>
    <div class="actions">
      <button class="btn-cancel" onclick={() => step = "peers"}>Back</button>
      <button class="btn-primary" onclick={confirmSend}>Send</button>
    </div>

  {:else if step === "sending"}
    <h2>Waiting for recipient…</h2>
    <p class="muted">Tell the recipient to enter this code:</p>
    <div class="code-display">{pairingCode.slice(0, 3)}-{pairingCode.slice(3)}</div>
    <p class="muted" style="font-size: 0.8rem;">Sending to: <strong>{selectedPeer?.name}</strong></p>

  {:else if step === "done"}
    <h2>Delivered</h2>
    <p class="muted">The recipient needs to enter this code to unlock {noteIds.length === 1 ? "the note" : `the ${noteIds.length} notes`}:</p>
    <div class="code-display">{pairingCode.slice(0, 3)}-{pairingCode.slice(3)}</div>
    <div class="actions">
      <button class="btn-primary" onclick={onclose}>Done</button>
    </div>

  {:else if step === "error"}
    <h2>Failed</h2>
    <p class="error">{errorMsg}</p>
    <div class="actions">
      <button class="btn-cancel" onclick={() => step = "peers"}>Try again</button>
      <button class="btn-primary" onclick={onclose}>Close</button>
    </div>
  {/if}
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0, 0, 0, 0.5);
  }
  .modal {
    position: fixed; z-index: 101;
    top: 50%; left: 50%; transform: translate(-50%, -50%);
    background: var(--surface); border: 1px solid var(--border);
    border-radius: 14px; padding: 1.75rem;
    width: min(420px, 92vw); max-height: 80vh;
    overflow-y: auto;
    padding-bottom: calc(1.75rem + env(safe-area-inset-bottom, 0px));
  }
  .close {
    position: absolute; top: 0.75rem; right: 1rem;
    background: none; border: none; font-size: 1.5rem;
    color: var(--muted); cursor: pointer; line-height: 1;
  }
  h2 { margin: 0 0 1rem; font-size: 1.1rem; }
  .section-label { font-size: 0.75rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.4rem; }
  .muted { color: var(--muted); font-size: 0.9rem; margin: 0.25rem 0; }
  .peer-list { list-style: none; margin: 0 0 0.5rem; padding: 0; display: flex; flex-direction: column; gap: 0.3rem; }
  .peer-item {
    width: 100%; display: flex; align-items: center; justify-content: space-between;
    padding: 0.6rem 0.85rem; border-radius: 8px;
    border: 1px solid var(--border); background: var(--bg);
    color: var(--text); cursor: pointer; text-align: left;
  }
  .peer-item:hover:not(:disabled) { background: var(--hover); }
  .peer-item.selected { border-color: var(--accent); background: var(--accent-muted); }
  .peer-item.dimmed { opacity: 0.45; cursor: not-allowed; }
  .peer-name { font-weight: 500; }
  .peer-via, .peer-meta { font-size: 0.75rem; color: var(--muted); }
  .rescan {
    font-size: 0.8rem; color: var(--accent); background: none;
    border: none; cursor: pointer; padding: 0;
  }
  .rescan:disabled { opacity: 0.5; cursor: not-allowed; }
  .code-display {
    font-size: 2.2rem; font-weight: 700; letter-spacing: 0.15em;
    text-align: center; padding: 1rem;
    background: var(--accent-muted); border-radius: 10px;
    color: var(--accent); margin: 1rem 0;
    font-family: monospace;
  }
  .actions { display: flex; gap: 0.75rem; justify-content: flex-end; margin-top: 1.25rem; }
  .btn-primary {
    padding: 0.55rem 1.25rem; border-radius: 8px;
    border: none; background: var(--accent); color: #fff;
    font-weight: 600; cursor: pointer;
  }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-cancel {
    padding: 0.55rem 1rem; border-radius: 8px;
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer;
  }
  .my-ips { font-size: 0.82rem; color: var(--muted); margin: 0.2rem 0 0.5rem; }
  .my-ips strong { color: var(--text); font-family: monospace; }
  .manual-row { display: flex; gap: 0.5rem; align-items: center; }
  .manual-input {
    flex: 1; padding: 0.5rem 0.75rem; border-radius: 8px;
    border: 1px solid var(--border); background: var(--input-bg);
    color: var(--text); font-size: 0.9rem; font-family: monospace;
  }
  .btn-connect {
    padding: 0.5rem 0.85rem; border-radius: 8px;
    border: none; background: var(--accent); color: #fff;
    font-weight: 600; cursor: pointer; font-size: 0.85rem; flex-shrink: 0;
  }
  .btn-connect:disabled { opacity: 0.5; cursor: not-allowed; }
  .manual-err { font-size: 0.78rem; color: #e74c3c; display: block; margin-top: 0.25rem; }
  .error { color: #e74c3c; font-size: 0.85rem; }
</style>
