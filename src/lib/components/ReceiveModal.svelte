<script lang="ts">
  import { onMount, onDestroy, tick } from "svelte";
  import { opticalImport, isReceiving, startReceiving, deviceIps } from "$lib/tauri";
  import { trapFocus } from "$lib/trapFocus";
  import { OpticalReceiver } from "$lib/optical/receiver";
  import { receiveTarget } from "$lib/stores/receiveTarget";
  import TransferTabs, { type TransferTab } from "./TransferTabs.svelte";
  import QrShowModal from "./QrShowModal.svelte";

  interface Props {
    /** Received notes land in this folder (their own subfolders nested inside); null is the root. */
    folderId?: string | null;
    folderName?: string;
    onclose: () => void;
  }
  let { folderId = null, folderName, onclose }: Props = $props();

  type Step = "scanning" | "passphrase" | "done" | "error";
  let tab = $state<TransferTab>("camera");
  let step = $state<Step>("scanning");
  let networkOn = $state(false);
  let networkBusy = $state(false);
  let networkError = $state("");
  let myIps = $state<string[]>([]);
  let showQr = $state(false);
  let solved = $state(0);
  let total = $state(0);
  let notice = $state("");
  let errorMsg = $state("");
  let passphrase = $state("");
  let passError = $state("");
  let importing = $state(false);
  let showPass = $state(false);
  let result = $state({ inserted: 0, updated: 0 });
  let payload: Uint8Array | null = null;
  let video: HTMLVideoElement | undefined = $state();
  let passInput: HTMLInputElement | undefined = $state();
  let receiver: OpticalReceiver | null = null;
  // Taken before the dialog moves focus, so closing returns it to the opener.
  const previouslyFocused = document.activeElement as HTMLElement | null;

  $effect(() => { if (step === "passphrase") passInput?.focus(); });

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") { e.preventDefault(); onclose(); }
  }

  async function startCamera() {
    if (!video) return;
    step = "scanning";
    solved = total = 0;
    notice = errorMsg = passphrase = passError = "";
    payload = null;
    receiver?.stop();
    const r = new OpticalReceiver(video, {
      progress: (s, k) => { solved = s; total = k; notice = ""; },
      complete: (bytes) => { payload = bytes; step = "passphrase"; },
      notice: (message) => { notice = message; },
      error: (message) => { errorMsg = message; step = "error"; },
    });
    receiver = r;
    try {
      await r.start();
    } catch (e) {
      r.stop();
      // Switched tab (or restarted) while the camera was starting: not ours to report.
      if (receiver !== r) return;
      errorMsg = e instanceof DOMException && e.name === "NotAllowedError"
        ? "Camera access was denied. Allow it for panote and try again."
        : `The camera could not start: ${e instanceof Error ? e.message : e}`;
      step = "error";
    }
  }

  async function submitPassphrase() {
    if (!payload || importing) return;
    importing = true;
    passError = "";
    try {
      result = await opticalImport(payload, passphrase, folderId);
      step = "done";
    } catch (e) {
      passError = String(e) === "wrong passphrase" ? "Wrong passphrase." : String(e);
    } finally {
      importing = false;
    }
  }

  /// The camera runs only on its own tab.
  async function selectTab(next: TransferTab) {
    tab = next;
    if (next === "camera") {
      await tick();
      void startCamera();
      return;
    }
    receiver?.stop();
    receiver = null;
    if (next === "network") {
      networkOn = await isReceiving().catch(() => false);
      myIps = await deviceIps().catch(() => []);
    }
  }

  async function turnOnNetwork() {
    networkBusy = true;
    networkError = "";
    try {
      await startReceiving();
      networkOn = true;
    } catch (e) {
      networkError = String(e);
    } finally {
      networkBusy = false;
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    // Transfers accepted from the notification while this is open land here too.
    receiveTarget.set({ id: folderId, name: folderName });
    void startCamera();
  });
  onDestroy(() => {
    receiver?.stop();
    receiveTarget.set(null);
    window.removeEventListener("keydown", onKey);
    previouslyFocused?.focus?.();
  });

  const into = $derived(folderName ?? "your notes");

  const received = $derived(result.inserted + result.updated);
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true" aria-labelledby="optical-title" use:trapFocus>
  <button class="close" onclick={onclose} aria-label="Close">
    <span class="material-symbols-outlined">close</span>
  </button>

  {#if step === "scanning" || step === "error"}
    <h2 id="optical-title">{folderName ? `Receive into ${folderName}` : "Receive"}</h2>
    <TransferTabs {tab} onselect={selectTab} />
  {/if}

  {#if tab === "bluetooth"}
    <p class="muted">Receiving over Bluetooth is coming soon. Use Camera, or Network on the same Wi-Fi.</p>
    <div class="actions">
      <button class="btn-cancel" onclick={onclose}>Close</button>
    </div>
  {:else if tab === "network"}
    {#if networkOn}
      <p class="muted">
        Receiving on this network{myIps.length ? ` as ${myIps.join(", ")}` : ""}. On the other device
        choose Send, Network tab, and pick this device.
      </p>
      <p class="muted">
        Incoming transfers appear as a notification. Accept them there while this is open and they
        land in {into}.
      </p>
      <div class="actions">
        <button class="btn-cancel" onclick={() => showQr = true}>Show my QR</button>
        <button class="btn-primary" onclick={onclose}>Done</button>
      </div>
    {:else}
      <p class="muted">Turn on receiving so devices on this Wi-Fi can find this one.</p>
      {#if networkError}<p class="error-text" role="alert">{networkError}</p>{/if}
      <div class="actions">
        <button class="btn-cancel" onclick={onclose}>Cancel</button>
        <button class="btn-primary" disabled={networkBusy} onclick={turnOnNetwork}>Start receiving</button>
      </div>
    {/if}
  {:else if step === "scanning"}
    <p class="muted">On the sending device choose Send, Camera tab, then point this camera at its codes.</p>
  {:else if step === "passphrase"}
    <h2 id="optical-title">Enter the passphrase</h2>
    <p class="muted">Received. Type the passphrase that was set on the sending device.</p>
  {:else if step === "done"}
    <h2 id="optical-title">Received</h2>
  {:else}
    <h2 id="optical-title">Could not receive</h2>
  {/if}

  {#if tab === "camera"}
  <!-- The video stays mounted so the receiver keeps its element across steps. -->
  <div class="camera" hidden={step !== "scanning"}>
    <video bind:this={video} muted playsinline></video>
  </div>

  {#if step === "scanning"}
    <div class="sr-only" role="status" aria-live="polite">
      {total ? `Receiving: ${solved} of ${total} parts.` : "Looking for codes."}
    </div>
    {#if total}
      <progress max={total} value={solved} aria-label="Received parts"></progress>
      <p class="status-text">{solved} of {total} parts</p>
    {:else}
      <p class="status-text">Looking for codes…</p>
    {/if}
    {#if notice}<p class="error-text">{notice}</p>{/if}
    <div class="actions">
      <button class="btn-cancel" onclick={onclose}>Cancel</button>
    </div>
  {:else if step === "passphrase"}
    <form onsubmit={(e) => { e.preventDefault(); submitPassphrase(); }}>
      <div class="pass-row">
        <input
          class="pass-input"
          type={showPass ? "text" : "password"}
          autocomplete="off"
          aria-label="Passphrase"
          placeholder="Passphrase"
          bind:value={passphrase}
          bind:this={passInput}
        />
        <button type="button" class="reveal" onclick={() => showPass = !showPass}
          aria-label={showPass ? "Hide passphrase" : "Show passphrase"} aria-pressed={showPass}>
          <span class="material-symbols-outlined" aria-hidden="true">{showPass ? "visibility_off" : "visibility"}</span>
        </button>
      </div>
      {#if passError}<p class="error-text" role="alert">{passError}</p>{/if}
      <div class="actions">
        <button type="button" class="btn-cancel" onclick={onclose}>Cancel</button>
        <button type="submit" class="btn-primary" disabled={importing || !passphrase}>
          {importing ? "Opening…" : "Open"}
        </button>
      </div>
    </form>
  {:else if step === "done"}
    <p class="muted" role="status">
      {received === 1 ? "1 note" : `${received} notes`} received{result.updated ? `, ${result.updated} updated in place` : ""}.
    </p>
    <div class="actions">
      <button class="btn-primary" onclick={onclose}>Done</button>
    </div>
  {:else}
    <p class="error-text" role="alert">{errorMsg}</p>
    <div class="actions">
      <button class="btn-cancel" onclick={onclose}>Close</button>
      <button class="btn-primary" onclick={startCamera}>Try again</button>
    </div>
  {/if}
  {/if}
</div>

{#if showQr}
  <QrShowModal onclose={() => showQr = false} />
{/if}

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
    width: min(440px, 92vw); max-height: 85vh;
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
  .camera { border-radius: var(--radius); overflow: hidden; margin: 0 auto 0.75rem; background: #000; }
  .camera[hidden] { display: none; }
  video { display: block; width: 100%; max-height: 50vh; object-fit: contain; }
  progress { width: 100%; accent-color: var(--accent); }
  .status-text { color: var(--accent); font-size: 0.9rem; font-weight: 600; margin: 0.5rem 0; }
  .error-text { color: var(--error); font-size: 0.85rem; margin: 0.5rem 0; }
  .pass-input {
    width: 100%; box-sizing: border-box; font: inherit;
    padding: 0.55rem 0.75rem; border-radius: var(--radius);
    border: 1px solid var(--border); background: var(--surface-container); color: var(--text);
  }
  .pass-row { position: relative; }
  .pass-row .pass-input { padding-right: 2.5rem; }
  .reveal {
    position: absolute; right: 0.35rem; top: 50%; transform: translateY(-50%);
    background: none; border: none; color: var(--muted); cursor: pointer;
    display: flex; padding: 0.25rem; border-radius: var(--radius-full);
  }
  .reveal:hover { color: var(--text); background: var(--hover); }
  .reveal .material-symbols-outlined { font-size: 20px; }
  .actions { display: flex; gap: 0.75rem; justify-content: center; margin-top: 1.25rem; }
  .btn-cancel {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--muted); cursor: pointer; transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--accent); color: var(--accent); }
  .btn-primary {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    cursor: pointer; font-weight: 600;
  }
  .btn-primary:disabled { opacity: 0.5; cursor: default; }
  .sr-only {
    position: absolute; width: 1px; height: 1px;
    padding: 0; margin: -1px; overflow: hidden;
    clip-path: inset(50%); white-space: nowrap; border: 0;
  }
</style>
