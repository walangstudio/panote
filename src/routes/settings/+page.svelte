<script lang="ts">
  import { onMount } from "svelte";
  import { getDeviceName, setDeviceName, startReceiving, stopReceiving, isReceiving, deviceIps, notesExport, notesImport, type ImportResolution, type ImportSummary } from "$lib/tauri";
  import { getVersion } from "@tauri-apps/api/app";
  import { theme, resolvedTheme } from "$lib/stores/theme";
  import { sidebarOpen } from "$lib/stores/sidebar";
  import QrShowModal from "$lib/components/QrShowModal.svelte";
  import ConfirmModal from "$lib/components/ConfirmModal.svelte";
  import PasswordModal from "$lib/components/PasswordModal.svelte";

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
  /// Set while waiting for the password that unseals protected notes in a backup.
  let sealedImportContents = $state<string | null>(null);
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

  /// A backup seals password-protected notes, so importing one needs that
  /// password before anything can be read.
  function fileHasSealedNotes(contents: string): boolean {
    try {
      const parsed = JSON.parse(contents);
      return Array.isArray(parsed?.notes) && parsed.notes.some((n: unknown) =>
        !!(n as { secret?: unknown })?.secret,
      );
    } catch {
      return false;
    }
  }

  function confirmImport() {
    const contents = pendingImportContents;
    if (!contents) return;
    pendingImportContents = null;
    if (fileHasSealedNotes(contents)) {
      // Collect the password first; the import runs from the modal's submit.
      sealedImportContents = contents;
      return;
    }
    void runImport(contents);
  }

  async function runImport(contents: string, secretPassword?: string) {
    importing = true;
    statusMessage = "";
    try {
      const summary: ImportSummary = await notesImport(contents, importResolution, secretPassword);
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

<div class="settings-page">
  <!-- Glass sticky header -->
  <div class="settings-header">
    <button class="menu-btn" onclick={() => $sidebarOpen = true} aria-label="Open menu">
      <span class="material-symbols-outlined">menu</span>
    </button>
    <span class="header-title">Settings</span>
  </div>

  <div class="settings-body">

    <!-- Device -->
    <div class="settings-group">
      <div class="group-label">Device</div>
      <div class="group-card">
        <div class="row">
          <span class="row-icon"><span class="material-symbols-outlined">smartphone</span></span>
          <div class="row-body">
            <span class="row-title">Device name</span>
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
          {#if !editingName}
            <span class="material-symbols-outlined row-chevron">chevron_right</span>
          {/if}
        </div>
      </div>
    </div>

    <!-- LAN Transfer -->
    <div class="settings-group">
      <div class="group-label">LAN Transfer</div>
      <div class="group-card">
        <div class="row">
          <span class="row-icon"><span class="material-symbols-outlined">wifi_tethering</span></span>
          <div class="row-body">
            <span class="row-title">Receive notes</span>
            <span class="row-sub">{receiving ? "Active — other devices can send" : "Accept incoming notes on this network"}</span>
          </div>
          <button
            class="toggle-pill"
            class:on={receiving}
            role="switch"
            aria-checked={receiving}
            aria-label="Toggle receiving"
            onclick={toggleReceive}
          >
            <span class="toggle-knob"></span>
          </button>
        </div>
        {#if receiving && myIps.length > 0}
          <div class="row-divider"></div>
          <div class="row">
            <span class="row-icon"><span class="material-symbols-outlined">lan</span></span>
            <div class="row-body">
              <span class="row-title">IP Addresses</span>
              <span class="row-sub mono">{myIps.join(", ")}</span>
            </div>
          </div>
          <div class="row-divider"></div>
          <button class="row actionable" onclick={() => showQr = true}>
            <span class="row-icon"><span class="material-symbols-outlined">qr_code_2</span></span>
            <div class="row-body">
              <span class="row-title">Show QR code</span>
              <span class="row-sub">Let sender scan to connect</span>
            </div>
            <span class="material-symbols-outlined row-chevron">chevron_right</span>
          </button>
        {/if}
      </div>
    </div>

    <!-- Data -->
    <div class="settings-group">
      <div class="group-label">Data</div>
      <div class="group-card">
        <button class="row actionable" onclick={doExport} disabled={exporting}>
          <span class="row-icon"><span class="material-symbols-outlined">file_download</span></span>
          <div class="row-body">
            <span class="row-title">Export all notes</span>
            <span class="row-sub">{exporting ? "Exporting…" : "Download a backup JSON file"}</span>
          </div>
          <span class="material-symbols-outlined row-chevron">chevron_right</span>
        </button>
        <div class="row-divider"></div>
        <button class="row actionable" onclick={triggerImportPicker} disabled={importing}>
          <span class="row-icon"><span class="material-symbols-outlined">file_upload</span></span>
          <div class="row-body">
            <span class="row-title">Import from file</span>
            <span class="row-sub">{importing ? "Importing…" : "Restore notes from a backup"}</span>
          </div>
          <span class="material-symbols-outlined row-chevron">chevron_right</span>
        </button>
        {#if statusMessage}
          <div class="row-divider"></div>
          <div class="row">
            <span class="row-icon"><span class="material-symbols-outlined">info</span></span>
            <div class="row-body">
              <span class="row-sub">{statusMessage}</span>
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
      </div>
    </div>

    <!-- Appearance -->
    <div class="settings-group">
      <div class="group-label">Appearance</div>
      <div class="group-card">
        <div class="row">
          <span class="row-icon">
            <span class="material-symbols-outlined" style="font-variation-settings: 'FILL' 1;">
              {$resolvedTheme === "candy-dark" ? "dark_mode" : "light_mode"}
            </span>
          </span>
          <div class="row-body">
            <label class="row-title" for="theme-select">Theme</label>
            <span class="row-sub">{$resolvedTheme === "candy-dark" ? "Candy dark" : "Candy light"}</span>
          </div>
          <select id="theme-select" class="theme-select" bind:value={$theme}>
            <option value="candy-light">Light</option>
            <option value="candy-dark">Dark</option>
            <option value="system">System</option>
          </select>
        </div>
      </div>
    </div>

    <!-- About -->
    <div class="settings-group">
      <div class="group-label">About</div>
      <div class="group-card">
        <div class="row">
          <span class="row-icon"><span class="material-symbols-outlined" style="font-variation-settings: 'FILL' 1;">info</span></span>
          <div class="row-body">
            <span class="row-title">Panote</span>
            <span class="row-sub">{appVersion ? `Version ${appVersion} · offline-first, encrypted` : "Loading…"}</span>
          </div>
        </div>
      </div>
    </div>

  </div>
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

{#if sealedImportContents !== null}
  <PasswordModal
    mode="unlock"
    title="Password-protected notes"
    onsubmit={async (v) => {
      const contents = sealedImportContents!;
      sealedImportContents = null;
      await runImport(contents, v.password);
    }}
    onclose={() => (sealedImportContents = null)}
  />
{/if}

<style>
  .settings-page {
    min-height: 100%;
  }

  /* ── Glass sticky header ── */
  .settings-header {
    position: sticky;
    top: 0;
    z-index: 15;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0.7rem 0.8rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface-glass);
    backdrop-filter: blur(16px);
    -webkit-backdrop-filter: blur(16px);
  }

  .menu-btn {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--text-secondary);
    border-radius: var(--radius-full);
    flex-shrink: 0;
    transition: color 0.15s ease, background 0.15s ease;
  }
  .menu-btn:hover { color: var(--accent); background: var(--accent-muted); }
  .menu-btn .material-symbols-outlined { font-size: 22px; }

  .header-title {
    font-size: 1.1rem;
    font-weight: 700;
    color: var(--text);
  }

  /* ── Body ── */
  .settings-body {
    padding: 1.1rem 0.9rem 3rem;
  }

  /* ── Group ── */
  .settings-group {
    margin-bottom: 22px;
  }

  .group-label {
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
    padding: 0 1.1rem;
    margin-bottom: 8px;
  }

  .group-card {
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: 0 4px 16px var(--shadow-color);
    overflow: hidden;
  }

  /* ── Row divider ── */
  .row-divider {
    height: 1px;
    background: var(--border);
    margin-left: calc(1rem + 36px);
  }

  /* ── Row ── */
  .row {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 0.85rem 1rem;
    cursor: default;
  }

  button.row,
  .row.actionable {
    width: 100%;
    text-align: left;
    background: none;
    border: none;
    cursor: pointer;
    font-family: inherit;
    transition: background 0.1s ease;
  }
  button.row:hover,
  .row.actionable:hover { background: var(--hover); }
  button.row:disabled,
  .row.actionable:disabled { opacity: 0.55; cursor: default; }

  .row-icon {
    font-size: 22px;
    color: var(--text-secondary);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    width: 22px;
  }
  .row-icon .material-symbols-outlined { font-size: 22px; }

  .row-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .row-title {
    font-size: 0.92rem;
    font-weight: 600;
    color: var(--text);
  }

  .row-sub {
    font-size: 0.78rem;
    color: var(--muted);
    margin-top: 1px;
  }

  .row-chevron {
    font-size: 20px;
    color: var(--muted);
    flex-shrink: 0;
  }

  /* ── Toggle pill (40×22) ── */
  .toggle-pill {
    width: 40px;
    height: 22px;
    border-radius: 11px;
    border: none;
    cursor: pointer;
    padding: 0;
    position: relative;
    flex-shrink: 0;
    background: var(--surface-container);
    transition: background 0.15s ease;
  }
  .toggle-pill.on { background: var(--accent); }

  .toggle-knob {
    position: absolute;
    top: 3px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-full);
    background: var(--muted);
    transition: left 0.15s ease, background 0.15s ease;
  }
  .toggle-pill.on .toggle-knob {
    left: 20px;
    background: var(--on-accent);
  }

  /* ── Theme select (Appearance row) ── */
  .theme-select {
    padding: 0.35rem 0.8rem;
    border-radius: var(--radius-full);
    border: 1px solid var(--border);
    cursor: pointer;
    background: transparent;
    color: var(--accent);
    font-family: inherit;
    font-size: 0.82rem;
    font-weight: 600;
    flex-shrink: 0;
    transition: background 0.1s ease;
  }
  .theme-select:hover { background: var(--accent-muted); }

  /* ── Device name inline edit ── */
  .name-input {
    padding: 0.35rem 0.6rem;
    font-size: 0.85rem;
    border: 1px solid var(--accent);
    border-radius: var(--radius-full);
    background: var(--input-bg);
    color: var(--text);
    outline: none;
    width: 100%;
    max-width: 220px;
    margin-top: 2px;
  }

  .name-value {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--accent);
    font-size: 0.85rem;
    text-align: left;
    font-weight: 500;
    margin-top: 1px;
  }
  .name-value:hover { text-decoration: underline; }

  .mono { font-family: monospace; font-size: 0.8rem; }

  @media (max-width: 640px) {
    .settings-body { padding: 0.9rem 0.6rem calc(2rem + env(safe-area-inset-bottom, 0px)); }
  }
</style>
