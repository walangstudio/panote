<script lang="ts">
  import { onMount } from "svelte";
  import QRCode from "qrcode";
  import { deviceIps, getDeviceName } from "$lib/tauri";

  interface Props {
    onclose: () => void;
  }
  let { onclose }: Props = $props();

  let canvas: HTMLCanvasElement;
  let ips = $state<string[]>([]);
  let selectedIp = $state("");
  let name = $state("");
  const port = 47291;

  onMount(async () => {
    ips = await deviceIps().catch(() => []);
    name = await getDeviceName().catch(() => "");
    if (ips.length > 0) selectedIp = ips[0];
  });

  $effect(() => {
    if (!selectedIp || !canvas) return;
    const payload = JSON.stringify({ ip: selectedIp, port, name });
    QRCode.toCanvas(canvas, payload, {
      width: 220,
      margin: 2,
      color: { dark: "#000000", light: "#ffffff" },
    });
  });
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true">
  <button class="close" onclick={onclose} aria-label="Close">
    <span class="material-symbols-outlined">close</span>
  </button>

  <h2>Show to sender</h2>
  <p class="muted">Let the other device scan this QR code to connect.</p>

  <div class="qr-wrap">
    <canvas bind:this={canvas}></canvas>
  </div>

  {#if selectedIp}
    <p class="info">{selectedIp}:{port}</p>
  {/if}
  {#if name}
    <p class="info device-name">{name}</p>
  {/if}

  {#if ips.length > 1}
    <div class="ip-select">
      <label class="section-label" for="ip-picker">Network interface</label>
      <select id="ip-picker" bind:value={selectedIp}>
        {#each ips as ip}
          <option value={ip}>{ip}</option>
        {/each}
      </select>
    </div>
  {/if}

  <div class="actions">
    <button class="btn-cancel" onclick={onclose}>Close</button>
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
    width: min(380px, 92vw); max-height: 80vh;
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
  .qr-wrap {
    display: flex; justify-content: center; margin: 0.5rem 0 1rem;
    background: #fff; border-radius: var(--radius); padding: 1rem;
    width: fit-content; margin-inline: auto;
  }
  .info {
    font-size: 0.85rem; color: var(--text-secondary); margin: 0.15rem 0;
    font-family: monospace;
  }
  .device-name { font-family: inherit; font-weight: 600; color: var(--text); }
  .ip-select { margin-top: 1rem; text-align: left; }
  .section-label {
    font-size: 0.75rem; color: var(--muted); text-transform: uppercase;
    letter-spacing: 0.05em; font-weight: 600; display: block; margin-bottom: 0.3rem;
  }
  .ip-select select {
    width: 100%; padding: 0.5rem 0.75rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: var(--input-bg);
    color: var(--text); font-size: 0.85rem; font-family: monospace;
  }
  .actions { display: flex; gap: 0.75rem; justify-content: center; margin-top: 1.25rem; }
  .btn-cancel {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--accent); color: var(--accent); }
</style>
