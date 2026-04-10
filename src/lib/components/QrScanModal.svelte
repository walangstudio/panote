<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Html5Qrcode } from "html5-qrcode";
  import { peerAddManual, type Peer } from "$lib/tauri";

  interface Props {
    onpeer: (peer: Peer) => void;
    onclose: () => void;
  }
  let { onpeer, onclose }: Props = $props();

  let status = $state<"scanning" | "connecting" | "error">("scanning");
  let errorMsg = $state("");
  let scanner: Html5Qrcode | null = null;
  const readerId = "qr-reader-" + Math.random().toString(36).slice(2, 8);

  onMount(async () => {
    scanner = new Html5Qrcode(readerId);
    try {
      await scanner.start(
        { facingMode: "environment" },
        { fps: 10, qrbox: { width: 220, height: 220 } },
        onDecode,
        () => {},
      );
    } catch (e) {
      errorMsg = "Camera access denied or unavailable.";
      status = "error";
    }
  });

  onDestroy(() => {
    scanner?.stop().catch(() => {});
  });

  async function onDecode(text: string) {
    if (status !== "scanning") return;
    let payload: { ip?: string; port?: number; name?: string };
    try {
      payload = JSON.parse(text);
    } catch {
      errorMsg = "Invalid QR code.";
      status = "error";
      return;
    }
    if (!payload.ip) {
      errorMsg = "QR code missing IP address.";
      status = "error";
      return;
    }

    status = "connecting";
    await scanner?.stop().catch(() => {});

    try {
      const peer = await peerAddManual(payload.ip);
      onpeer(peer);
    } catch (e) {
      errorMsg = `Connection failed: ${e}`;
      status = "error";
    }
  }

  function retry() {
    errorMsg = "";
    status = "scanning";
    scanner?.start(
      { facingMode: "environment" },
      { fps: 10, qrbox: { width: 220, height: 220 } },
      onDecode,
      () => {},
    ).catch(() => {
      errorMsg = "Could not restart camera.";
      status = "error";
    });
  }
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true">
  <button class="close" onclick={onclose} aria-label="Close">
    <span class="material-symbols-outlined">close</span>
  </button>

  <h2>Scan QR code</h2>
  <p class="muted">Point the camera at the recipient's QR code.</p>

  <div class="reader-wrap">
    <div id={readerId} class="reader"></div>
  </div>

  {#if status === "connecting"}
    <p class="status-text">Connecting...</p>
  {:else if status === "error"}
    <p class="error-text">{errorMsg}</p>
    <button class="btn-retry" onclick={retry}>
      <span class="material-symbols-outlined" style="font-size: 18px;">refresh</span>
      Try again
    </button>
  {/if}

  <div class="actions">
    <button class="btn-cancel" onclick={onclose}>Cancel</button>
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
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg); padding: 1.75rem;
    width: min(400px, 92vw); max-height: 85vh;
    overflow-y: auto;
    box-shadow: 0 16px 48px var(--shadow-color-hover);
    text-align: center;
    padding-bottom: calc(1.75rem + env(safe-area-inset-bottom, 0px));
  }
  .close {
    position: absolute; top: 0.75rem; right: 0.75rem;
    background: var(--accent-muted); border: none; border-radius: var(--radius-full);
    color: var(--muted); cursor: pointer;
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .close:hover { background: var(--accent); color: var(--on-accent); }
  h2 { margin: 0 0 0.5rem; font-size: 1.1rem; font-weight: 700; }
  .muted { color: var(--muted); font-size: 0.85rem; margin: 0 0 1rem; }
  .reader-wrap {
    border-radius: var(--radius); overflow: hidden;
    margin: 0 auto 1rem; max-width: 300px;
  }
  .reader { width: 100%; min-height: 250px; background: #000; border-radius: var(--radius); }
  :global(#qr-reader video) { border-radius: var(--radius); }
  .status-text { color: var(--accent); font-size: 0.9rem; font-weight: 600; margin: 0.5rem 0; }
  .error-text { color: var(--error); font-size: 0.85rem; margin: 0.5rem 0; }
  .btn-retry {
    display: inline-flex; align-items: center; gap: 0.4rem;
    padding: 0.45rem 1rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--accent); cursor: pointer; font-size: 0.85rem; font-weight: 600;
    transition: all 0.15s ease;
  }
  .btn-retry:hover { border-color: var(--accent); background: var(--accent-muted); }
  .actions { display: flex; gap: 0.75rem; justify-content: center; margin-top: 1.25rem; }
  .btn-cancel {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--accent); color: var(--accent); }
</style>
