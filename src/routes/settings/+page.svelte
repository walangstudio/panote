<script lang="ts">
  import { onMount } from "svelte";
  import { getDeviceName, setDeviceName, startReceiving, stopReceiving, isReceiving, deviceIps, notesExport, notesImport, type ImportResolution, type ImportSummary } from "$lib/tauri";
  import { getVersion } from "@tauri-apps/api/app";
  import { toggleDarkMode, theme } from "$lib/stores/theme";
  import { sidebarOpen } from "$lib/stores/sidebar";
  import QrShowModal from "$lib/components/QrShowModal.svelte";
  import ConfirmModal from "$lib/components/ConfirmModal.svelte";

  let appVersion = $state("");
  let deviceName = $state("");
  let editingName = $state(false);
  let nameInput = $state("");
  let receiving = $state(false);
  let myIps = $state<string[]>([]);
  let showQr = $state(false);

  let exporting = $state(false);
  let importing = $state(false);
  let pendingImportContents = $state<string | null>(null);
  let importResolution = $state<ImportResolution>("overwrite");
  let statusMessage = $state("");
  let fileInput = $state<HTMLInputElement | null>(null);

  async function doExport() {
    if (exporting) return;
    exporting = true;
    statusMessage = "";
    try {
      const json = await notesExport(appVersion || "unknown");
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      const stamp = new Date().toISOString().slice(0, 10);
      a.href = url;
      a.download = `panote-backup-${stamp}.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      statusMessage = "Backup downloaded.";
    } catch (e) {
      statusMessage = `Export failed: ${e}`;
    } finally {
      exporting = false;
    }
  }

  function triggerImportPicker() {
    fileInput?.click();
  }

  async function onFilePicked(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = "";
    if (!file) return;
    try {
      pendingImportContents = await file.text();
      importResolution = "overwrite";
    } catch (err) {
      statusMessage = `Could not read file: ${err}`;
    }
  }

  async function confirmImport() {
    const contents = pendingImportContents;
    pendingImportContents = null;
    if (!contents) return;
    importing = true;
    statusMessage = "";
    try {
      const summary: ImportSummary = await notesImport(contents, importResolution);
      const parts: string[] = [];
      if (summary.imported) parts.push(`${summary.imported} new`);
      if (summary.updated) parts.push(`${summary.updated} updated`);
      if (summary.skipped) parts.push(`${summary.skipped} skipped`);
      if (summary.errors.length) parts.push(`${summary.errors.length} errors`);
      statusMessage = parts.length ? `Imported: ${parts.join(", ")}.` : "Nothing to import.";
    } catch (e) {
      statusMessage = `Import failed: ${e}`;
    } finally {
      importing = false;
    }
  }

  function cancelImport() {
    pendingImportContents = null;
  }

  onMount(async () => {
    appVersion = await getVersion();
    try { deviceName = await getDeviceName(); } catch {}
    try { receiving = await isReceiving(); } catch {}
    try { myIps = await deviceIps(); } catch {}
  });

  async function saveName() {
    const trimmed = nameInput.trim();
    if (!trimmed || trimmed === deviceName) { editingName = false; return; }
    try {
      await setDeviceName(trimmed);
      deviceName = trimmed;
    } catch {}
    editingName = false;
  }

  async function toggleReceive() {
    try {
      if (receiving) {
        await stopReceiving();
        receiving = false;
      } else {
        await startReceiving();
        receiving = true;
      }
    } catch {}
  }
</script>

<div class="page">
  <div class="page-header">
    <button class="menu-btn" onclick={() => $sidebarOpen = true} aria-label="Open menu">
      <span class="material-symbols-outlined">menu</span>
    </button>
    <h1>Settings</h1>
  </div>

  <section class="card">
    <h2 class="section-title">Appearance</h2>
    <button class="setting-row" onclick={toggleDarkMode}>
      <span class="setting-icon">
        <span class="material-symbols-outlined">{$theme === "candy-dark" ? "light_mode" : "dark_mode"}</span>
      </span>
      <div class="setting-text">
        <span class="setting-label">{$theme === "candy-dark" ? "Switch to light mode" : "Switch to dark mode"}</span>
        <span class="setting-desc">Currently using {$theme === "candy-dark" ? "dark" : "light"} theme</span>
      </div>
      <span class="material-symbols-outlined chevron">chevron_right</span>
    </button>
  </section>

  <section class="card">
    <h2 class="section-title">Device</h2>
    <div class="setting-row">
      <span class="setting-icon">
        <span class="material-symbols-outlined">smartphone</span>
      </span>
      <div class="setting-text">
        <span class="setting-label">Device name</span>
        {#if editingName}
          <!-- svelte-ignore a11y_autofocus -->
          <input
            class="name-input"
            bind:value={nameInput}
            onkeydown={(e) => { if (e.key === "Enter") saveName(); if (e.key === "Escape") editingName = false; }}
            onblur={saveName}
            autofocus
          />
        {:else}
          <button class="name-value" onclick={() => { nameInput = deviceName; editingName = true; }}>
            {deviceName || "Tap to set"}
          </button>
        {/if}
      </div>
    </div>
  </section>

  <section class="card">
    <h2 class="section-title">Transfer</h2>
    <button class="setting-row" onclick={toggleReceive}>
      <span class="setting-icon">
        <span class="material-symbols-outlined">download</span>
      </span>
      <div class="setting-text">
        <span class="setting-label">Receive notes</span>
        <span class="setting-desc">{receiving ? "Active — other devices can send" : "Off"}</span>
      </div>
      <span class="toggle-pill" class:active={receiving}>
        <span class="toggle-knob"></span>
      </span>
    </button>
    {#if receiving && myIps.length > 0}
      <div class="setting-row">
        <span class="setting-icon">
          <span class="material-symbols-outlined">lan</span>
        </span>
        <div class="setting-text">
          <span class="setting-label">IP Addresses</span>
          <span class="setting-desc mono">{myIps.join(", ")}</span>
        </div>
      </div>
      <button class="setting-row" onclick={() => showQr = true}>
        <span class="setting-icon">
          <span class="material-symbols-outlined">qr_code_2</span>
        </span>
        <div class="setting-text">
          <span class="setting-label">Show QR code</span>
          <span class="setting-desc">Let sender scan to connect</span>
        </div>
        <span class="material-symbols-outlined chevron">chevron_right</span>
      </button>
    {/if}
  </section>

  <section class="card">
    <h2 class="section-title">Data</h2>
    <button class="setting-row" onclick={doExport} disabled={exporting}>
      <span class="setting-icon">
        <span class="material-symbols-outlined">file_download</span>
      </span>
      <div class="setting-text">
        <span class="setting-label">Export all notes</span>
        <span class="setting-desc">{exporting ? "Exporting…" : "Download a backup JSON file"}</span>
      </div>
      <span class="material-symbols-outlined chevron">chevron_right</span>
    </button>
    <button class="setting-row" onclick={triggerImportPicker} disabled={importing}>
      <span class="setting-icon">
        <span class="material-symbols-outlined">file_upload</span>
      </span>
      <div class="setting-text">
        <span class="setting-label">Import from file</span>
        <span class="setting-desc">{importing ? "Importing…" : "Restore notes from a backup"}</span>
      </div>
      <span class="material-symbols-outlined chevron">chevron_right</span>
    </button>
    {#if statusMessage}
      <div class="setting-row">
        <span class="setting-icon">
          <span class="material-symbols-outlined">info</span>
        </span>
        <div class="setting-text">
          <span class="setting-desc">{statusMessage}</span>
        </div>
      </div>
    {/if}
    <input
      bind:this={fileInput}
      type="file"
      accept="application/json,.json"
      style="display:none"
      onchange={onFilePicked}
    />
  </section>

  <section class="card">
    <h2 class="section-title">About</h2>
    <div class="setting-row">
      <span class="setting-icon">
        <span class="material-symbols-outlined">info</span>
      </span>
      <div class="setting-text">
        <span class="setting-label">Panote</span>
        <span class="setting-desc">{appVersion ? `Version ${appVersion}` : "Loading…"}</span>
      </div>
    </div>
  </section>
</div>

{#if showQr}
  <QrShowModal onclose={() => showQr = false} />
{/if}

{#if pendingImportContents !== null}
  <ConfirmModal
    title="Import notes?"
    message="Existing notes with the same origin will be overwritten. Notes new to this device will be added."
    confirmLabel="Overwrite &amp; import"
    cancelLabel="Cancel"
    onconfirm={confirmImport}
    oncancel={cancelImport}
  />
{/if}

<style>
  .page {
    padding: 1.5rem 2rem 2rem;
    max-width: 600px; margin: 0 auto;
  }
  .page-header {
    display: flex; align-items: center; gap: 0.75rem; margin-bottom: 1.5rem;
  }
  .menu-btn {
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    display: flex; align-items: center; padding: 0.25rem; border-radius: var(--radius-full);
    flex-shrink: 0; transition: all 0.15s ease;
  }
  .menu-btn:hover { color: var(--accent); background: var(--accent-muted); }
  h1 {
    font-size: 1.5rem; font-weight: 900; margin: 0;
    color: var(--text);
  }
  .card {
    background: var(--surface); border-radius: var(--radius);
    border: 1px solid var(--border);
    box-shadow: 0 4px 16px var(--shadow-color);
    margin-bottom: 1rem; overflow: hidden;
  }
  .section-title {
    font-size: 0.75rem; color: var(--muted); text-transform: uppercase;
    letter-spacing: 0.05em; font-weight: 600;
    padding: 0.75rem 1rem 0; margin: 0;
  }
  .setting-row {
    display: flex; align-items: center; gap: 0.75rem;
    padding: 0.85rem 1rem; width: 100%; text-align: left;
    background: none; border: none; cursor: pointer;
    color: var(--text); transition: background 0.1s ease;
  }
  .setting-row:hover { background: var(--hover); }
  .setting-icon {
    width: 40px; height: 40px; border-radius: 12px;
    background: var(--accent-surface); color: var(--accent);
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0;
  }
  .setting-text { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .setting-label { font-weight: 600; font-size: 0.9rem; }
  .setting-desc { font-size: 0.78rem; color: var(--muted); }
  .mono { font-family: monospace; font-size: 0.8rem; }
  .chevron { color: var(--muted); margin-left: auto; }

  .toggle-pill {
    width: 40px; height: 22px; border-radius: 11px;
    background: var(--surface-container); border: 1px solid var(--border);
    position: relative; flex-shrink: 0;
    transition: all 0.2s ease;
  }
  .toggle-pill.active { background: var(--accent); border-color: var(--accent); }
  .toggle-knob {
    position: absolute; top: 2px; left: 2px;
    width: 16px; height: 16px; border-radius: 50%;
    background: var(--muted);
    transition: all 0.2s ease;
  }
  .toggle-pill.active .toggle-knob {
    left: 20px; background: var(--on-accent);
  }

  .name-input {
    padding: 0.35rem 0.6rem; font-size: 0.85rem;
    border: 1px solid var(--accent); border-radius: var(--radius-full);
    background: var(--input-bg); color: var(--text); outline: none;
    width: 100%; max-width: 220px;
  }
  .name-value {
    background: none; border: none; padding: 0; cursor: pointer;
    color: var(--accent); font-size: 0.85rem; text-align: left;
    font-weight: 500;
  }
  .name-value:hover { text-decoration: underline; }

  @media (max-width: 640px) {
    .page { padding: 1rem 0.75rem calc(1rem + env(safe-area-inset-bottom, 0px)); }
    h1 { font-size: 1.3rem; }
  }
</style>
