<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import {
    peersScan, notesSend, generatePairingCode, knownPeersList, peerAddManual, deviceIps,
    noteList, noteUnlock, opticalPack,
    type Peer, type KnownPeer,
  } from "$lib/tauri";
  import { trapFocus } from "$lib/trapFocus";
  import { frameSource, playStream } from "$lib/optical/sender";
  import QrShowModal from "./QrShowModal.svelte";
  import QrScanModal from "./QrScanModal.svelte";

  interface Props {
    noteIds: string[];
    onclose: () => void;
  }
  let { noteIds, onclose }: Props = $props();

  type Step = "peers" | "code" | "screen-pass" | "unlock" | "sending" | "streaming" | "done" | "error";

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
  let showQr = $state(false);
  let scanQr = $state(false);
  let closeBtn: HTMLButtonElement | undefined = $state();
  let unlockInput: HTMLInputElement | undefined = $state();
  let previouslyFocused: HTMLElement | null = null;

  // Screen transfer: the passphrase is typed, never shown, because anything on
  // this screen is visible to whoever films the stream.
  let optical = $state(false);
  let screenPass = $state("");
  let screenPassInput: HTMLInputElement | undefined = $state();
  let streamCanvas: HTMLCanvasElement | undefined = $state();
  let stopStream: (() => void) | null = null;
  let packing = $state(false);
  let showScreenPass = $state(false);
  let destroyed = false;
  // Same floor as optical.rs MIN_PASSPHRASE_CHARS: a filmed stream can be
  // attacked offline, unlike a LAN pairing code.
  const MIN_SCREEN_PASS = 10;

  $effect(() => { if (step === "unlock") unlockInput?.focus(); });
  $effect(() => { if (step === "screen-pass") screenPassInput?.focus(); });

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onclose(); }
  }

  // Protected notes in this selection must be unlocked before we can send them
  // (Model B: the note is decrypted on this device, sent inside the E2E envelope).
  let protectedQueue = $state<{ id: string; title: string }[]>([]);
  let unlockIdx = $state(0);
  let unlockPw = $state("");
  let unlockError = $state("");
  let unlockBusy = $state(false);

  onMount(async () => {
    previouslyFocused = document.activeElement as HTMLElement | null;
    closeBtn?.focus();
    window.addEventListener("keydown", onKey);
    recentPeers = await knownPeersList().catch(() => []);
    myIps = await deviceIps().catch(() => []);
    const metas = await noteList().catch(() => []);
    const chosen = new Set(noteIds);
    protectedQueue = metas
      .filter((m) => chosen.has(m.id) && m.has_note_password)
      .map((m) => ({ id: m.id, title: m.title }));
    await scan();
  });
  onDestroy(() => {
    destroyed = true;
    stopStream?.();
    window.removeEventListener("keydown", onKey);
    previouslyFocused?.focus?.();
  });

  function chooseScreen() {
    optical = true;
    screenPass = "";
    step = "screen-pass";
  }

  function backToPeers() {
    stopStream?.();
    stopStream = null;
    optical = false;
    step = "peers";
  }

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
    if (!optical && !selectedPeer) return;
    if (optical && (packing || [...screenPass].length < MIN_SCREEN_PASS)) return;
    // Unlock any protected notes first, one at a time (labeled by title).
    if (protectedQueue.length > 0) {
      unlockIdx = 0;
      unlockPw = "";
      unlockError = "";
      step = "unlock";
      return;
    }
    await doSend();
  }

  async function submitUnlock() {
    const cur = protectedQueue[unlockIdx];
    if (!cur || unlockBusy) return;
    unlockBusy = true;
    unlockError = "";
    try {
      await noteUnlock(cur.id, unlockPw);
      unlockPw = "";
      unlockIdx += 1;
      if (unlockIdx >= protectedQueue.length) {
        await doSend();
      }
    } catch {
      unlockError = "Wrong password for this note.";
    } finally {
      unlockBusy = false;
    }
  }

  async function doSend() {
    if (optical) return startStream();
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

  async function startStream() {
    if (packing) return;
    packing = true;
    try {
      const source = await frameSource(await opticalPack(noteIds, screenPass));
      screenPass = "";
      if (destroyed) return; // closed while sealing: nothing left to paint on
      step = "streaming";
      await tick();
      stopStream?.();
      if (streamCanvas && !destroyed) stopStream = playStream(streamCanvas, source);
    } catch (e) {
      errorMsg = String(e);
      step = "error";
    } finally {
      packing = false;
    }
  }

  async function connectManual() {
    const ip = manualIp.trim();
    if (!ip) return;
    manualBusy = true;
    manualError = "";
    try {
      const peer = await peerAddManual(ip);
      // Dedupe on what came back, not on what was typed - the input may carry a
      // port, and the same host on two ports is two different peers.
      livePeers = [
        ...livePeers.filter(p => !(p.address === peer.address && p.port === peer.port)),
        peer,
      ];
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

  const stepAnnouncement = $derived(
    ({
      peers: "Choose a device to send to.",
      code: "Pairing code ready. Share it with the recipient.",
      unlock: `Unlock note ${unlockIdx + 1} of ${protectedQueue.length} to continue.`,
      sending: "Waiting for the recipient to enter the code.",
      "screen-pass": "Choose a passphrase for the screen transfer.",
      streaming: "Showing the notes as moving QR codes.",
      done: "Transfer delivered.",
      error: "Transfer failed.",
    } as const)[step],
  );
</script>

<div class="overlay">
  <div class="backdrop" role="presentation" onclick={onclose}></div>
  <div class="modal" role="dialog" aria-modal="true" use:trapFocus>
    <button class="close" bind:this={closeBtn} onclick={onclose} aria-label="Close">
      <span class="material-symbols-outlined">close</span>
    </button>
    <div class="sr-only" role="status" aria-live="polite">{stepAnnouncement}</div>

    {#if step === "peers"}
      <h2>Transfer over LAN</h2>
      <p class="desc">Have the other device scan this code, or pick a peer on your network. Nothing leaves your LAN.</p>

      <div class="qr-actions">
        <button class="qr-btn" onclick={() => showQr = true}>
          <span class="material-symbols-outlined">qr_code_2</span>
          Show my QR
        </button>
        <button class="qr-btn" onclick={() => scanQr = true}>
          <span class="material-symbols-outlined">qr_code_scanner</span>
          Scan QR code
        </button>
        <button class="qr-btn" onclick={chooseScreen}>
          <span class="material-symbols-outlined">screen_share</span>
          Send by screen
        </button>
      </div>

      <div class="section-label" style="margin-top: 1rem;">Nearby devices</div>
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
          placeholder="e.g. 192.168.1.42 or 192.168.1.42:47291"
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

      <div class="sending-card">
        <div class="sending-label">SENDING</div>
        <div class="sending-value">
          {noteIds.length === 1 ? "1 note" : `${noteIds.length} notes`} → <strong>{selectedPeer?.name}</strong>
        </div>
      </div>

      <p class="muted" style="font-size: 0.78rem;">
        Notes are end-to-end encrypted with this code. They arrive as normal notes —
        the recipient can choose to protect them.
      </p>

      <div class="actions">
        <button class="btn-cancel" onclick={() => step = "peers"}>Back</button>
        <button class="btn-primary" onclick={confirmSend}>Send to peer</button>
      </div>

    {:else if step === "screen-pass"}
      <h2>Send by screen</h2>
      <p class="muted">
        This screen plays the {noteIds.length === 1 ? "note" : `${noteIds.length} notes`} as moving QR codes
        for the other device's camera. No network needed.
      </p>
      <form onsubmit={(e) => { e.preventDefault(); confirmSend(); }}>
        <label class="section-label" for="screen-pass">Passphrase</label>
        <div class="pass-row">
          <input
            id="screen-pass"
            class="manual-input pass-input"
            type={showScreenPass ? "text" : "password"}
            autocomplete="off"
            placeholder="At least {MIN_SCREEN_PASS} characters"
            bind:value={screenPass}
            bind:this={screenPassInput}
          />
          <button type="button" class="reveal" onclick={() => showScreenPass = !showScreenPass}
            aria-label={showScreenPass ? "Hide passphrase" : "Show passphrase"} aria-pressed={showScreenPass}>
            <span class="material-symbols-outlined" aria-hidden="true">{showScreenPass ? "visibility_off" : "visibility"}</span>
          </button>
        </div>
        <p class="muted hint">
          Type the same passphrase on the receiving device. Anyone who films the
          screen sees the codes, so the notes are encrypted with it.
        </p>
        <div class="actions">
          <button type="button" class="btn-cancel" onclick={backToPeers}>Back</button>
          <button type="submit" class="btn-primary" disabled={packing || [...screenPass].length < MIN_SCREEN_PASS}>{packing ? "Encrypting…" : "Start"}</button>
        </div>
      </form>

    {:else if step === "streaming"}
      <h2>Show this to the camera</h2>
      <p class="muted">
        On the other device open Settings, Receive by camera, and point it here.
        Keep this open until that device says it received the notes.
      </p>
      <canvas class="stream" bind:this={streamCanvas} aria-label="Animated QR codes carrying the notes"></canvas>
      <div class="actions">
        <button class="btn-primary" onclick={onclose}>Done</button>
      </div>

    {:else if step === "unlock"}
      <h2>Unlock to send</h2>
      <p class="muted">
        Enter the password for this protected note ({unlockIdx + 1} of {protectedQueue.length}):
      </p>
      <div class="sending-card">
        <div class="sending-label">NOTE</div>
        <div class="sending-value"><strong>{protectedQueue[unlockIdx]?.title || "Untitled"}</strong></div>
      </div>
      <form onsubmit={(e) => { e.preventDefault(); submitUnlock(); }}>
        <input
          class="manual-input"
          style="font-family: inherit; width: 100%; margin-top: 0.75rem;"
          type="password"
          placeholder="Note password"
          bind:value={unlockPw}
          bind:this={unlockInput}
        />
        {#if unlockError}<p class="muted" style="color: var(--danger, #e5484d);">{unlockError}</p>{/if}
        <div class="actions">
          <button type="button" class="btn-cancel" onclick={() => step = optical ? "screen-pass" : "code"}>Cancel</button>
          <button type="submit" class="btn-primary" disabled={unlockBusy || !unlockPw}>Unlock</button>
        </div>
      </form>

    {:else if step === "sending"}
      <h2>Waiting for recipient…</h2>
      <p class="muted">Tell the recipient to enter this code:</p>
      <div class="code-display">{pairingCode.slice(0, 3)}-{pairingCode.slice(3)}</div>

      <div class="sending-card">
        <div class="sending-label">SENDING</div>
        <div class="sending-value">
          {noteIds.length === 1 ? "1 note" : `${noteIds.length} notes`} → <strong>{selectedPeer?.name}</strong>
        </div>
      </div>

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
        <button class="btn-cancel" onclick={backToPeers}>Try again</button>
        <button class="btn-primary" onclick={onclose}>Close</button>
      </div>
    {/if}
  </div>
</div>

{#if showQr}
  <QrShowModal onclose={() => showQr = false} />
{/if}

{#if scanQr}
  <QrScanModal
    onclose={() => scanQr = false}
    onpeer={(peer) => {
      scanQr = false;
      livePeers = [...livePeers.filter(p => p.address !== peer.address), peer];
      selectedPeer = peer;
    }}
  />
{/if}

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
    border-radius: var(--radius-lg); padding: 1.75rem;
    width: min(420px, 92%); max-height: 80vh;
    overflow-y: auto;
    box-shadow: 0 16px 48px var(--shadow-color-hover);
    padding-bottom: calc(1.75rem + env(safe-area-inset-bottom, 0px));
    animation: panote-pop-in 0.18s ease;
  }
  .close {
    position: absolute; top: 0.75rem; right: 0.75rem;
    background: var(--accent-muted); border: none; border-radius: var(--radius-full);
    color: var(--muted); cursor: pointer;
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .close:hover { background: var(--accent); color: var(--on-accent); }
  h2 { margin: 0 0 0.8rem; font-size: 1.1rem; font-weight: 700; }
  .desc {
    margin: 0 0 1rem; color: var(--text-secondary); font-size: 0.88rem; line-height: 1.5;
  }
  .section-label { font-size: 0.75rem; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; margin-bottom: 0.4rem; font-weight: 600; }
  .muted { color: var(--muted); font-size: 0.9rem; margin: 0.25rem 0; }
  .peer-list { list-style: none; margin: 0 0 0.5rem; padding: 0; display: flex; flex-direction: column; gap: 0.3rem; }
  .peer-item {
    width: 100%; display: flex; align-items: center; justify-content: space-between;
    padding: 0.65rem 0.95rem; border-radius: var(--radius);
    border: 1px solid var(--border); background: var(--surface);
    color: var(--text); cursor: pointer; text-align: left;
    transition: all 0.15s ease;
  }
  .peer-item:hover:not(:disabled) { background: var(--hover); box-shadow: 0 2px 8px var(--shadow-color); }
  .peer-item.selected { border-color: var(--accent); background: var(--accent-muted); }
  .peer-item.dimmed { opacity: 0.45; cursor: not-allowed; }
  .peer-name { font-weight: 600; }
  .peer-via, .peer-meta { font-size: 0.75rem; color: var(--muted); }
  .rescan {
    font-size: 0.8rem; color: var(--accent); background: none;
    border: none; cursor: pointer; padding: 0; font-weight: 600;
  }
  .rescan:disabled { opacity: 0.5; cursor: not-allowed; }
  .code-display {
    font-size: 2.2rem; font-weight: 700; letter-spacing: 0.15em;
    text-align: center; padding: 1rem;
    background: var(--accent-muted); border-radius: var(--radius);
    color: var(--accent); margin: 1rem 0;
    font-family: monospace;
  }
  .sending-card {
    background: var(--surface-container); border-radius: var(--radius);
    padding: 0.7rem 0.9rem; margin-bottom: 1rem;
  }
  .sending-label {
    font-size: 0.72rem; color: var(--muted); font-weight: 600; margin-bottom: 4px;
  }
  .sending-value { font-size: 0.9rem; font-weight: 700; }
  .actions { display: flex; gap: 0.75rem; justify-content: flex-end; margin-top: 1.25rem; }
  .btn-primary {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; cursor: pointer; font-family: inherit;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease;
  }
  .btn-primary:hover { transform: scale(1.03); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; transform: none; }
  .btn-cancel {
    padding: 0.55rem 1rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-family: inherit;
    transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--accent); color: var(--accent); }
  .my-ips { font-size: 0.82rem; color: var(--muted); margin: 0.2rem 0 0.5rem; }
  .my-ips strong { color: var(--text); font-family: monospace; }
  .manual-row { display: flex; gap: 0.5rem; align-items: center; }
  .manual-input {
    flex: 1; padding: 0.5rem 0.75rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: var(--input-bg);
    color: var(--text); font-size: 0.9rem; font-family: monospace;
  }
  .btn-connect {
    padding: 0.5rem 0.85rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; cursor: pointer; font-size: 0.85rem; flex-shrink: 0;
    transition: transform 0.1s ease;
  }
  .btn-connect:hover { transform: scale(1.03); }
  .btn-connect:disabled { opacity: 0.5; cursor: not-allowed; transform: none; }
  .manual-err { font-size: 0.78rem; color: var(--error); display: block; margin-top: 0.25rem; }
  .error { color: var(--error); font-size: 0.85rem; }
  .qr-actions {
    display: flex; flex-wrap: wrap; gap: 0.5rem; margin-bottom: 0.75rem;
  }
  .pass-row .pass-input { font-family: inherit; width: 100%; box-sizing: border-box; padding-right: 2.5rem; }
  .pass-row { position: relative; margin-top: 0.35rem; }
  .reveal {
    position: absolute; right: 0.35rem; top: 50%; transform: translateY(-50%);
    background: none; border: none; color: var(--muted); cursor: pointer;
    display: flex; padding: 0.25rem; border-radius: var(--radius-full);
  }
  .reveal:hover { color: var(--text); background: var(--hover); }
  .reveal .material-symbols-outlined { font-size: 20px; }
  .hint { font-size: 0.78rem; }
  /* White quiet zone around the codes even in the dark theme: cameras need it. */
  .stream {
    display: block; width: min(100%, 60vh); aspect-ratio: 1;
    margin: 0.75rem auto; background: #fff; border-radius: 4px;
  }
  .qr-btn {
    flex: 1; display: flex; align-items: center; justify-content: center; gap: 0.4rem;
    padding: 0.55rem 0.75rem; border-radius: var(--radius);
    border: 1px solid var(--border); background: var(--surface-container);
    color: var(--text-secondary); cursor: pointer; font-size: 0.82rem; font-weight: 600;
    transition: all 0.15s ease;
  }
  .qr-btn:hover { border-color: var(--accent); color: var(--accent); background: var(--accent-muted); }
  .qr-btn .material-symbols-outlined { font-size: 20px; }

  .sr-only {
    position: absolute; width: 1px; height: 1px;
    padding: 0; margin: -1px; overflow: hidden;
    clip-path: inset(50%); white-space: nowrap; border: 0;
  }
</style>
