<script lang="ts">
  import { page } from "$app/state";
  import { onMount } from "svelte";
  import { goto, beforeNavigate } from "$app/navigation";
  import {
    noteGet, noteCreate, noteUpdate,
    noteUnlock, noteLock, noteProtect, noteUnprotect, noteChangePassword,
    noteRecover, noteAddRecovery,
    LOCKED, type NoteKind,
  } from "$lib/tauri";
  import { refreshNotes } from "$lib/stores/notes";
  import { recordNoteSaved } from "$lib/gamekit/store";
  import ConfirmModal from "$lib/components/ConfirmModal.svelte";
  import PasswordModal from "$lib/components/PasswordModal.svelte";
  import MarkdownEditor from "$lib/components/MarkdownEditor.svelte";
  import ChecklistEditor from "$lib/components/ChecklistEditor.svelte";
  import KanbanEditor from "$lib/components/KanbanEditor.svelte";
  import TableEditor from "$lib/components/TableEditor.svelte";
  import { sidebarOpen } from "$lib/stores/sidebar";
  import { detectFormat } from "$lib/detectFormat";

  const id = $derived(page.params.id ?? "");
  const isNew = $derived(id === "new");
  const kindParam = $derived((page.url.searchParams.get("kind") ?? "document") as NoteKind);
  const modeParam = $derived(page.url.searchParams.get("mode"));

  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let hasPassword = $state(false);
  let locked = $state(false);

  type PwMode = "set" | "change" | "remove";
  let pwModal = $state<{ mode: PwMode } | null>(null);
  let needUnlockForSave = $state(false);
  let recoverOpen = $state(false);
  // Password just used to protect, so we can offer a recovery code without re-asking.
  let postProtectPw = $state<string | null>(null);
  // One-time recovery code to display (never stored anywhere but the user's copy).
  let recoveryCode = $state<string | null>(null);
  let recoveryBusy = $state(false);

  let kind = $state<NoteKind>("document");
  let title = $state("");
  let content = $state<unknown>({});
  let tags = $state<string[]>([]);
  let tagInput = $state("");
  let menuOpen = $state(false);
  let transferOpen = $state(false);
  let showPreview = $state(true);
  let bgColor = $state<string | undefined>();
  let bgImage = $state<string | undefined>();
  let bgMenuOpen = $state(false);
  let savedTitle = $state("");
  let savedContent = $state("{}");
  let savedTags = $state("[]");
  let justSaved = $state(false);
  let pendingNavUrl = $state<string | null>(null);
  let updatedAt = $state<number | undefined>();

  // Auto-contrast ink for custom backgrounds
  let imgInkResolved = $state<"dark" | "light" | null>(null);

  // ponytail: defense in depth — backend already validates bg_image is a data:image/... URI.
  function safeBgImageUrl(bgImage: string | undefined): string | undefined {
    return bgImage && bgImage.startsWith("data:image/") ? `url(${bgImage})` : undefined;
  }

  function isLightColor(c: string): boolean {
    const hex = c.trim().match(/^#?([0-9a-f]{3}|[0-9a-f]{6})$/i);
    let r: number, g: number, b: number;
    if (hex) {
      let h = hex[1];
      if (h.length === 3) h = h.split("").map(x => x + x).join("");
      r = parseInt(h.slice(0, 2), 16); g = parseInt(h.slice(2, 4), 16); b = parseInt(h.slice(4, 6), 16);
    } else {
      const m = c.match(/(\d+)[,\s]+(\d+)[,\s]+(\d+)/);
      if (!m) return true;
      r = +m[1]; g = +m[2]; b = +m[3];
    }
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.6;
  }

  function analyzeImageInk(url: string) {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      try {
        const c = document.createElement("canvas");
        c.width = 16; c.height = 16;
        const ctx = c.getContext("2d")!;
        ctx.drawImage(img, 0, 0, 16, 16);
        const d = ctx.getImageData(0, 0, 16, 16).data;
        let sum = 0;
        for (let i = 0; i < d.length; i += 4) sum += 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
        imgInkResolved = sum / (d.length / 4) / 255 > 0.6 ? "dark" : "light";
      } catch { imgInkResolved = "dark"; }
    };
    img.onerror = () => { imgInkResolved = "dark"; };
    img.src = url;
  }

  $effect(() => {
    if (bgImage) {
      analyzeImageInk(bgImage);
    } else {
      imgInkResolved = null;
    }
  });

  const editorInk = $derived((): "dark" | "light" | null => {
    if (bgColor) return isLightColor(bgColor) ? "dark" : "light";
    if (bgImage) return imgInkResolved ?? "dark";
    return null;
  });

  const dirty = $derived(
    !justSaved && (
      title !== savedTitle ||
      JSON.stringify(content) !== savedContent ||
      JSON.stringify(tags) !== savedTags
    )
  );

  beforeNavigate(({ cancel, to }) => {
    if (!dirty || pendingNavUrl !== null) return;
    cancel();
    pendingNavUrl = to?.url?.toString() ?? "";
  });

  function discardAndNavigate() {
    const target = pendingNavUrl;
    pendingNavUrl = null;
    justSaved = true;
    if (target) {
      goto(target);
    } else {
      history.back();
    }
  }

  onMount(async () => {
    if (isNew) {
      kind = kindParam;
      content = defaultContent(kind);
      savedTitle = title;
      savedContent = JSON.stringify(content);
      savedTags = JSON.stringify(tags);
      loading = false;
      return;
    }
    await loadNote();
  });

  async function loadNote() {
    try {
      const note = await noteGet(id);
      kind = note.kind;
      title = note.title;
      content = note.content;
      tags = note.tags;
      showPreview = note.show_preview;
      bgColor = note.bg_color ?? undefined;
      bgImage = note.bg_image ?? undefined;
      hasPassword = note.has_note_password;
      updatedAt = note.updated_at;
      savedTitle = title;
      savedContent = JSON.stringify(content);
      savedTags = JSON.stringify(tags);
      locked = false;
      error = "";
    } catch (e) {
      if (String(e) === LOCKED) {
        locked = true;
        hasPassword = true;
      } else {
        error = String(e);
      }
    }
    loading = false;
  }

  async function unlock(v: { password: string }) {
    await noteUnlock(id, v.password);
    await loadNote();
  }

  async function handlePassword(v: { password: string; oldPassword?: string }) {
    if (!pwModal) return;
    const m = pwModal.mode;
    if (m === "set") await noteProtect(id, v.password);
    else if (m === "change") await noteChangePassword(id, v.oldPassword ?? "", v.password);
    else await noteUnprotect(id, v.password);
    hasPassword = m !== "remove";
    // After protecting, offer a recovery code (reusing the password we just set).
    if (m === "set") postProtectPw = v.password;
    await refreshNotes();
  }

  async function handleRecover(v: { password: string; recoveryCode?: string }) {
    await noteRecover(id, v.recoveryCode ?? "", v.password);
    recoverOpen = false;
    await loadNote();
  }

  async function generateRecovery() {
    if (!postProtectPw || recoveryBusy) return;
    recoveryBusy = true;
    try {
      recoveryCode = await noteAddRecovery(id, postProtectPw);
    } finally {
      postProtectPw = null;
      recoveryBusy = false;
    }
  }

  async function lockButtonClick() {
    if (!hasPassword) {
      pwModal = { mode: "set" };
    } else {
      await noteLock(id);
      justSaved = true;
      goto("/");
    }
  }

  async function save() {
    addTag();
    saving = true;
    error = "";
    try {
      const content_hint = kind === "document" ? detectFormat((content as { body: string }).body ?? "") : undefined;
      const input = { kind, title, content, tags, content_hint, show_preview: showPreview, bg_color: bgColor, bg_image: bgImage };
      if (isNew) {
        await noteCreate(input);
      } else {
        await noteUpdate(id, input);
      }
      // Gamification — never let a tracking error block the save.
      try { await recordNoteSaved({ isNew, kind, content }); } catch (e) { console.error("gamekit", e); }
      await refreshNotes();
      justSaved = true;
      goto("/");
    } catch (e) {
      if (String(e) === LOCKED) {
        needUnlockForSave = true;
      } else {
        error = String(e);
      }
    }
    saving = false;
  }

  async function unlockForSave(v: { password: string }) {
    await noteUnlock(id, v.password);
    needUnlockForSave = false;
    await save();
  }

  function addTag() {
    const parts = tagInput.split(",").map(t => t.trim()).filter(t => t && !tags.includes(t));
    if (parts.length) tags = [...tags, ...parts];
    tagInput = "";
  }

  function removeTag(t: string) {
    tags = tags.filter(x => x !== t);
  }

  function handleBgImageUpload(e: Event) {
    const input = e.target as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    if (file.size > 2 * 1024 * 1024) {
      error = "Image too large. Maximum size is 2MB.";
      input.value = "";
      return;
    }
    const reader = new FileReader();
    reader.onload = () => {
      const img = new Image();
      img.onload = () => {
        const maxDim = 1920;
        let { width, height } = img;
        if (width > maxDim || height > maxDim) {
          const ratio = Math.min(maxDim / width, maxDim / height);
          width = Math.round(width * ratio);
          height = Math.round(height * ratio);
        }
        const canvas = document.createElement("canvas");
        canvas.width = width;
        canvas.height = height;
        const ctx = canvas.getContext("2d")!;
        ctx.drawImage(img, 0, 0, width, height);
        bgImage = canvas.toDataURL("image/jpeg", 0.8);
        bgColor = undefined;
      };
      img.src = reader.result as string;
    };
    reader.readAsDataURL(file);
    input.value = "";
  }

  function defaultContent(k: NoteKind): unknown {
    if (k === "document") return { body: "" };
    if (k === "checklist") return { items: [] };
    if (k === "kanban") return { columns: [{ id: crypto.randomUUID(), name: "To do", cards: [] }] };
    if (k === "table") return { columns: [], rows: [] };
    return {};
  }

  function formatRelative(unixSecs: number): string {
    const HOUR = 3_600_000, DAY = 24 * HOUR;
    const diff = Date.now() - unixSecs * 1000;
    if (diff < HOUR) return Math.max(1, Math.round(diff / 60_000)) + "m";
    if (diff < DAY) return Math.round(diff / HOUR) + "h";
    if (diff < 7 * DAY) return Math.round(diff / DAY) + "d";
    return new Date(unixSecs * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
  }

  // Channel colors by kind (mirrors home screen kindColor map)
  const kindChannel: Record<string, "accent" | "secondary" | "tertiary"> = {
    document: "accent",
    table: "secondary",
    checklist: "tertiary",
    kanban: "tertiary",
  };
  const kindIconName: Record<string, string> = {
    document: "edit_note",
    table: "table_chart",
    checklist: "checklist",
    kanban: "view_kanban",
  };
  const kindLabel: Record<string, string> = {
    document: "Document",
    table: "Table",
    checklist: "Checklist",
    kanban: "Kanban",
  };

  const BG_SWATCHES: (string | null)[] = [
    null, "#ffe3f1", "#ffe9c7", "#fff7c2", "#d9f5e0", "#cdeeff", "#e7dcff", "#f3dcec",
  ];

  // Title textarea auto-grow
  function autoGrowTitle(node: HTMLTextAreaElement) {
    function resize() {
      node.style.height = "auto";
      node.style.height = node.scrollHeight + "px";
    }
    node.addEventListener("input", resize);
    resize();
    return { destroy() { node.removeEventListener("input", resize); } };
  }

  // LockGate inline state
  let lockPw = $state("");
  let lockPwErr = $state(false);

  async function inlineUnlock() {
    if (!lockPw) { lockPwErr = true; return; }
    try {
      await noteUnlock(id, lockPw);
      lockPw = "";
      lockPwErr = false;
      await loadNote();
    } catch {
      lockPwErr = true;
    }
  }
</script>

{#if loading}
  <div class="loading">Loading…</div>
{:else if locked}
  <div class="lock-gate">
    <div class="lock-gate-circle">
      <span class="material-symbols-outlined" style="font-size: 36px; font-variation-settings: 'FILL' 1;">lock</span>
    </div>
    <div class="lock-gate-text">
      <div class="lock-gate-title">This note is locked</div>
      <div class="lock-gate-sub">Enter the password to view it.</div>
    </div>
    <input
      type="password"
      class="lock-gate-input"
      class:error={lockPwErr}
      placeholder="Password"
      bind:value={lockPw}
      autofocus
      oninput={() => lockPwErr = false}
      onkeydown={(e) => { if (e.key === "Enter") inlineUnlock(); }}
    />
    <button class="unlock-btn" onclick={inlineUnlock}>
      <span class="material-symbols-outlined" style="font-size: 20px; font-variation-settings: 'wght' 600;">lock_open</span>
      Unlock
    </button>
    <button class="recover-link" onclick={() => recoverOpen = true}>Forgot password? Recover with code</button>
  </div>
{:else}
  <div
    class="editor-layout"
    class:dark-ink={editorInk() === "dark"}
    class:light-ink={editorInk() === "light"}
    class:has-bg-image={!!bgImage}
    style:background-color={bgColor}
    style:background-image={safeBgImageUrl(bgImage)}
    style:background-size={bgImage ? "cover" : undefined}
    style:background-position={bgImage ? "center" : undefined}
  >
    <!-- Glass sticky header -->
    <header class="editor-header">
      <a href="/" class="round-icon" aria-label="Back">
        <span class="material-symbols-outlined" style="font-size: 20px;">arrow_back</span>
      </a>
      <div class="header-spacer"></div>
      <!-- Kind chip -->
      <div class="kind-chip {kindChannel[kind] ?? 'accent'}">
        <span class="material-symbols-outlined" style="font-size: 16px; font-variation-settings: 'FILL' 1;">{kindIconName[kind] ?? "edit_note"}</span>
        {kindLabel[kind] ?? kind}
      </div>
      <!-- more_vert overflow trigger -->
      {#if !isNew}
        <button
          class="bare-icon"
          class:active={menuOpen}
          onclick={() => menuOpen = !menuOpen}
          aria-label="More options"
        >
          <span class="material-symbols-outlined" style="font-size: 22px; font-variation-settings: 'FILL' {menuOpen ? 1 : 0};">more_vert</span>
        </button>
      {/if}
    </header>

    <!-- Overflow dropdown -->
    {#if menuOpen}
      <div class="menu-backdrop" role="presentation" onclick={() => menuOpen = false}></div>
      <div class="overflow-menu">
        <button class="overflow-item" onclick={() => { menuOpen = false; bgMenuOpen = true; }}>
          <span class="material-symbols-outlined" style="font-size: 20px;">palette</span>
          Background
        </button>
        <button class="overflow-item" onclick={() => { menuOpen = false; transferOpen = true; }}>
          <span class="material-symbols-outlined" style="font-size: 20px;">send</span>
          Transfer
        </button>
        {#if hasPassword}
          <button class="overflow-item" onclick={() => { menuOpen = false; pwModal = { mode: "change" }; }}>
            <span class="material-symbols-outlined" style="font-size: 20px;">password</span>
            Change password
          </button>
          <button class="overflow-item" onclick={() => { menuOpen = false; pwModal = { mode: "remove" }; }}>
            <span class="material-symbols-outlined" style="font-size: 20px;">lock_open</span>
            Remove password
          </button>
        {:else}
          <button class="overflow-item" onclick={() => { menuOpen = false; lockButtonClick(); }}>
            <span class="material-symbols-outlined" style="font-size: 20px;">lock</span>
            Set password
          </button>
        {/if}
        <div class="overflow-divider"></div>
        <button class="overflow-item danger" onclick={() => { menuOpen = false; }}>
          <span class="material-symbols-outlined" style="font-size: 20px;">delete</span>
          Delete
        </button>
      </div>
    {/if}

    <!-- Content area -->
    <div class="editor-content">
      <!-- Title -->
      <textarea
        class="title-input"
        placeholder="Untitled"
        bind:value={title}
        rows={1}
        use:autoGrowTitle
      ></textarea>

      <!-- Edited subline -->
      {#if updatedAt}
        <div class="edited-line">Edited {formatRelative(updatedAt)} ago</div>
      {/if}

      {#if error}<p class="error">{error}</p>{/if}

      <!-- Background sheet (inline, below title) -->
      {#if bgMenuOpen}
        <div class="bg-sheet">
          <div class="bg-sheet-header">
            <span class="bg-sheet-label">Background</span>
            <button class="bare-icon small" onclick={() => bgMenuOpen = false} aria-label="Close">
              <span class="material-symbols-outlined" style="font-size: 20px;">close</span>
            </button>
          </div>
          <div class="bg-swatches">
            {#each BG_SWATCHES as c}
              <button
                class="swatch"
                class:active={c === null ? (!bgColor && !bgImage) : bgColor === c}
                style:background-color={c ?? undefined}
                style:background={c === null ? "var(--surface)" : undefined}
                aria-label={c ?? "none"}
                onclick={() => { bgColor = c ?? undefined; bgImage = undefined; }}
              >
                {#if c === null}
                  <span class="material-symbols-outlined" style="font-size: 16px; color: var(--muted);">format_color_reset</span>
                {/if}
              </button>
            {/each}
            <!-- Image swatch -->
            <button
              class="swatch image-swatch"
              class:active={!!bgImage}
              aria-label="image"
              onclick={() => document.getElementById("bg-file-input-editor")?.click()}
            >
              <span class="material-symbols-outlined" style="font-size: 16px; color: var(--muted);">image</span>
            </button>
          </div>
          <input type="file" accept="image/png,image/jpeg,image/webp,image/gif"
            class="bg-file-input" id="bg-file-input-editor" onchange={handleBgImageUpload} />
          {#if bgImage}
            <div class="bg-preview-row">
              <span class="bg-preview-thumb" style:background-image={safeBgImageUrl(bgImage)}></span>
              <button class="bg-clear-btn" onclick={() => { bgImage = undefined; }}>
                <span class="material-symbols-outlined" style="font-size: 14px;">close</span>
                Remove
              </button>
            </div>
          {/if}
        </div>
      {/if}

      <!-- Body editors -->
      <div class="editor-body">
        {#if kind === "document"}
          <MarkdownEditor bind:content initialPreview={modeParam === "edit" ? false : modeParam === "view" ? true : !isNew} />
        {:else if kind === "checklist"}
          <ChecklistEditor bind:content />
        {:else if kind === "kanban"}
          <KanbanEditor bind:content />
        {:else if kind === "table"}
          <TableEditor bind:content />
        {/if}
      </div>

      <!-- Tags -->
      <div class="tags-row">
        {#each tags as t}
          <span class="tag-chip">
            #{t}
            <button class="tag-remove" onclick={() => removeTag(t)} aria-label="Remove tag">
              <span class="material-symbols-outlined" style="font-size: 13px;">close</span>
            </button>
          </span>
        {/each}
        <input
          class="tag-input"
          placeholder="+ tag"
          bind:value={tagInput}
          enterkeyhint="done"
          onkeydown={(e) => { if (e.key === "Enter" || e.key === ",") { e.preventDefault(); addTag(); } }}
          oninput={() => { if (tagInput.includes(",")) addTag(); }}
          onblur={addTag}
        />
      </div>
    </div>

    <!-- Footer -->
    <div class="editor-footer">
      <label class="preview-toggle">
        <input type="checkbox" bind:checked={showPreview} />
        <span>Show preview on list</span>
      </label>
      <div class="footer-spacer"></div>
      <button class="save-btn" onclick={save} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </button>
    </div>

    {#if transferOpen}
      {#await import("$lib/components/TransferModal.svelte") then { default: TransferModal }}
        <TransferModal noteIds={id ? [id] : []} onclose={() => transferOpen = false} />
      {/await}
    {/if}
  </div>
{/if}

{#if pendingNavUrl !== null}
  <ConfirmModal
    title="Unsaved changes"
    message="You have unsaved changes. Leave without saving?"
    confirmLabel="Discard"
    destructive
    onconfirm={discardAndNavigate}
    oncancel={() => pendingNavUrl = null}
  />
{/if}

{#if pwModal && !locked}
  <PasswordModal
    mode={pwModal.mode}
    onsubmit={handlePassword}
    onclose={() => pwModal = null}
  />
{/if}

{#if needUnlockForSave}
  <PasswordModal
    mode="unlock"
    title="Unlock to save"
    onsubmit={unlockForSave}
    onclose={() => needUnlockForSave = false}
  />
{/if}

{#if recoverOpen}
  <PasswordModal
    mode="recover"
    onsubmit={handleRecover}
    onclose={() => recoverOpen = false}
  />
{/if}

{#if postProtectPw}
  <div class="rc-overlay">
    <div class="rc-backdrop" role="presentation" onclick={() => postProtectPw = null}></div>
    <div class="rc-modal" role="dialog" aria-modal="true">
      <h2>Add a recovery code?</h2>
      <p class="rc-desc">
        A one-time code lets you recover this note if you forget the password.
        Store it in a password manager — anyone with it can open the note.
      </p>
      <div class="rc-actions">
        <button class="btn-cancel" onclick={() => postProtectPw = null}>Skip</button>
        <button class="btn-confirm" disabled={recoveryBusy} onclick={generateRecovery}>Generate code</button>
      </div>
    </div>
  </div>
{/if}

{#if recoveryCode}
  <div class="rc-overlay">
    <div class="rc-backdrop" role="presentation"></div>
    <div class="rc-modal" role="dialog" aria-modal="true">
      <h2>Your recovery code</h2>
      <p class="rc-desc">
        Save this now — it's shown once and never stored. Anyone with it can open this note.
      </p>
      <div class="rc-code">{recoveryCode}</div>
      <div class="rc-actions">
        <button class="btn-confirm" onclick={() => { navigator.clipboard?.writeText(recoveryCode ?? ""); }}>Copy</button>
        <button class="btn-confirm" onclick={() => recoveryCode = null}>Done</button>
      </div>
    </div>
  </div>
{/if}

<style>
  /* ── Loading ── */
  .loading {
    display: flex; align-items: center; justify-content: center;
    height: 100%; color: var(--muted);
  }

  /* ── Lock gate ── */
  .lock-gate {
    display: flex; flex-direction: column; align-items: center; justify-content: center;
    gap: 18px; padding: 3rem 1.5rem; text-align: center; min-height: 100%;
  }
  .lock-gate-circle {
    width: 72px; height: 72px; border-radius: var(--radius-full);
    background: var(--accent-muted); color: var(--accent);
    display: flex; align-items: center; justify-content: center;
  }
  .lock-gate-title { font-weight: 700; font-size: 1.05rem; margin-bottom: 4px; }
  .lock-gate-sub { color: var(--muted); font-size: 0.85rem; }
  .recover-link {
    background: none; border: none; color: var(--muted); cursor: pointer;
    font-family: inherit; font-size: 0.82rem; text-decoration: underline; padding: 4px;
  }
  .recover-link:hover { color: var(--accent); }
  .rc-overlay {
    position: fixed; inset: 0; z-index: 120;
    display: flex; align-items: center; justify-content: center; padding: 1.1rem;
  }
  .rc-backdrop {
    position: absolute; inset: 0; background: rgba(0, 0, 0, 0.45);
    backdrop-filter: blur(4px); -webkit-backdrop-filter: blur(4px);
  }
  .rc-modal {
    position: relative; z-index: 121; background: var(--surface-glass);
    backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius-lg);
    padding: 1.5rem 1.6rem; width: min(400px, 92%);
    box-shadow: 0 16px 48px var(--shadow-color-hover);
  }
  .rc-modal h2 { margin: 0 0 0.6rem; font-size: 1.1rem; font-weight: 700; color: var(--text); }
  .rc-desc { margin: 0 0 1rem; color: var(--text-secondary); font-size: 0.86rem; line-height: 1.5; }
  .rc-code {
    font-family: ui-monospace, monospace; font-size: 1.1rem; font-weight: 700;
    letter-spacing: 0.06em; text-align: center; padding: 0.8rem;
    background: var(--input-bg); border: 1px solid var(--border);
    border-radius: var(--radius); color: var(--text); margin-bottom: 1rem; user-select: all;
  }
  .rc-actions { display: flex; gap: 0.6rem; justify-content: flex-end; }
  .rc-actions .btn-cancel {
    padding: 0.55rem 1rem; border-radius: var(--radius-full); border: 1px solid var(--border);
    background: transparent; color: var(--muted); cursor: pointer; font-weight: 600; font-family: inherit;
  }
  .rc-actions .btn-confirm {
    padding: 0.55rem 1.25rem; border-radius: var(--radius-full); border: none;
    background: var(--accent); color: var(--on-accent); font-weight: 600; cursor: pointer; font-family: inherit;
  }
  .rc-actions .btn-confirm:disabled { opacity: 0.5; cursor: not-allowed; }
  .lock-gate-input {
    width: 100%; max-width: 260px; padding: 0.7rem 1rem; text-align: center;
    border-radius: var(--radius-full);
    border: 1px solid var(--border);
    background: var(--input-bg); color: var(--text);
    font-family: inherit; font-size: 0.95rem; outline: none;
    transition: border-color 0.15s ease;
  }
  .lock-gate-input:focus { border-color: var(--accent); }
  .lock-gate-input.error { border-color: var(--error); }
  .unlock-btn {
    display: inline-flex; align-items: center; justify-content: center; gap: 8px;
    padding: 0.7rem 1.2rem; border: none; cursor: pointer;
    border-radius: var(--radius-full); font-family: inherit;
    font-size: 0.95rem; font-weight: 700;
    background: var(--accent); color: var(--on-accent);
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.12s ease;
  }
  .unlock-btn:hover { transform: scale(1.03); }

  /* ── Layout ── */
  .editor-layout {
    display: flex; flex-direction: column; min-height: 100%;
    position: relative;
  }
  .editor-layout.has-bg-image {
    background-size: cover; background-position: center;
  }
  /* Overlay tint for image backgrounds so content stays legible */
  .editor-layout.has-bg-image.dark-ink::before {
    content: ""; position: fixed; inset: 0; pointer-events: none; z-index: 0;
    background: rgba(255,255,255,0.45);
  }
  .editor-layout.has-bg-image.light-ink::before {
    content: ""; position: fixed; inset: 0; pointer-events: none; z-index: 0;
    background: rgba(0,0,0,0.4);
  }
  .editor-layout > * { position: relative; z-index: 1; }

  /* ── Glass sticky header ── */
  .editor-header {
    position: sticky; top: 0; z-index: 15;
    display: flex; align-items: center; gap: 8px;
    padding: 0.7rem 0.8rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface-glass);
    backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
  }
  .header-spacer { flex: 1; }

  /* RoundIcon — 36px circle, accent-muted bg, accent color */
  .round-icon {
    width: 36px; height: 36px; border-radius: var(--radius-full);
    display: flex; align-items: center; justify-content: center;
    background: var(--accent-muted); color: var(--accent);
    text-decoration: none; flex-shrink: 0;
    transition: background 0.15s ease, color 0.15s ease;
    border: none; cursor: pointer;
  }
  .round-icon:hover { background: var(--accent); color: var(--on-accent); }

  /* Kind chip pill */
  .kind-chip {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 0.3rem 0.7rem 0.3rem 0.5rem;
    border-radius: var(--radius-full);
    font-size: 0.78rem; font-weight: 700;
    white-space: nowrap; flex-shrink: 0;
  }
  .kind-chip.accent   { background: var(--accent-surface);    color: var(--accent); }
  .kind-chip.secondary { background: var(--secondary-surface); color: var(--secondary); }
  .kind-chip.tertiary  { background: var(--tertiary-surface);  color: var(--tertiary); }

  /* BareIcon — no border/bg, hover → accent-muted pill */
  .bare-icon {
    width: 40px; height: 40px;
    display: flex; align-items: center; justify-content: center;
    border-radius: var(--radius-full); border: none; cursor: pointer;
    background: transparent; color: var(--text-secondary);
    transition: background 0.15s ease, color 0.15s ease;
    flex-shrink: 0;
  }
  .bare-icon:hover { background: var(--accent-muted); color: var(--accent); }
  .bare-icon.active { color: var(--accent); }
  .bare-icon.small { width: 28px; height: 28px; }

  /* ── Overflow dropdown ── */
  .menu-backdrop { position: fixed; inset: 0; z-index: 19; }
  .overflow-menu {
    position: absolute; top: 56px; right: 12px; z-index: 20;
    width: 220px;
    background: var(--surface-glass);
    backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    padding: 6px;
    display: flex; flex-direction: column; gap: 2px;
  }
  .overflow-item {
    display: flex; align-items: center; gap: 12px;
    width: 100%; text-align: left;
    padding: 0.6rem 0.7rem; border: none; background: transparent; cursor: pointer;
    border-radius: var(--radius-sm);
    font-family: inherit; font-size: 0.9rem; font-weight: 500;
    color: var(--text-secondary);
    transition: background 0.1s ease;
  }
  .overflow-item:hover { background: var(--hover); }
  .overflow-item.danger { color: var(--error); }
  .overflow-divider { height: 1px; background: var(--border); margin: 4px 6px; }

  /* ── Content area ── */
  .editor-content {
    flex: 1; padding: 1.1rem 1.25rem 2rem;
    display: flex; flex-direction: column;
  }

  /* Title textarea */
  .title-input {
    width: 100%; border: none; outline: none; background: transparent; resize: none;
    font-family: inherit; font-weight: 900; font-size: 1.45rem; line-height: 1.2;
    color: var(--text); margin-bottom: 4px; overflow: hidden;
    min-height: 0;
  }
  .title-input::placeholder { color: var(--muted); }

  /* Edited subline */
  .edited-line {
    font-size: 0.78rem; font-weight: 500; color: var(--muted);
    margin-bottom: 18px;
  }

  /* Error */
  .error { color: var(--error); font-size: 0.85rem; margin: 0 0 0.5rem; }

  /* ── Background sheet ── */
  .bg-sheet {
    margin-bottom: 14px; padding: 0.9rem 1rem;
    border-radius: var(--radius);
    background: var(--surface-container);
  }
  .bg-sheet-header {
    display: flex; align-items: center; justify-content: space-between;
    margin-bottom: 10px;
  }
  .bg-sheet-label { font-size: 0.85rem; font-weight: 700; color: var(--text-secondary); }
  .bg-swatches { display: flex; gap: 8px; flex-wrap: wrap; }
  .swatch {
    width: 34px; height: 34px; border-radius: var(--radius-full);
    cursor: pointer; border: 1px solid rgba(0,0,0,0.08);
    display: flex; align-items: center; justify-content: center;
    transition: border 0.1s ease, box-shadow 0.1s ease;
    flex-shrink: 0;
  }
  .swatch:first-child { border: 1px solid var(--border); }
  .swatch.active { border: 2px solid var(--accent); box-shadow: 0 0 0 2px var(--accent-muted); }
  .image-swatch {
    background: repeating-linear-gradient(
      45deg, var(--surface), var(--surface) 4px,
      var(--surface-high) 4px, var(--surface-high) 8px
    );
    border: 1px solid var(--border);
  }
  .bg-file-input { display: none; }
  .bg-preview-row { display: flex; align-items: center; gap: 0.5rem; margin-top: 8px; }
  .bg-preview-thumb {
    width: 40px; height: 28px; border-radius: 4px;
    background-size: cover; background-position: center;
    border: 1px solid var(--border);
  }
  .bg-clear-btn {
    display: flex; align-items: center; gap: 0.25rem;
    background: none; border: none; cursor: pointer;
    color: var(--error); font-size: 0.75rem; font-weight: 500;
    padding: 0.2rem 0.4rem; border-radius: var(--radius-sm);
    transition: background 0.1s ease;
  }
  .bg-clear-btn:hover { background: var(--error-surface); }

  /* ── Editor body ── */
  .editor-body { flex: 1; }

  /* ── Tags ── */
  .tags-row {
    display: flex; flex-wrap: wrap; gap: 8px; align-items: center;
    margin-top: 26px;
  }
  .tag-chip {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 0.25em 0.7em; border-radius: var(--radius-full);
    font-size: 0.78rem; font-weight: 600;
    background: var(--accent-muted); color: var(--accent);
  }
  .tag-remove {
    background: none; border: none; cursor: pointer; color: inherit;
    padding: 0; display: flex; align-items: center;
  }
  .tag-input {
    border: none; background: transparent; outline: none;
    font-family: inherit; font-size: 0.82rem; color: var(--muted);
    width: 70px;
  }
  .tag-input::placeholder { color: var(--muted); }

  /* ── Footer ── */
  .editor-footer {
    display: flex; align-items: center; gap: 0.75rem;
    padding: 0.5rem 1rem 0.5rem 1.25rem;
    border-top: 1px solid var(--border);
    background: var(--surface-container);
    flex-shrink: 0;
  }
  .footer-spacer { flex: 1; }
  .preview-toggle {
    display: flex; align-items: center; gap: 0.4rem;
    font-size: 0.78rem; color: var(--text-secondary); cursor: pointer; white-space: nowrap;
  }
  .preview-toggle input { accent-color: var(--accent); cursor: pointer; }
  .save-btn {
    padding: 0.5rem 1.25rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 700; cursor: pointer;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease;
  }
  .save-btn:hover { transform: scale(1.03); }
  .save-btn:disabled { opacity: 0.6; cursor: not-allowed; transform: none; }

  /* ── Auto-contrast ink ── */
  .editor-layout.dark-ink .title-input { color: #2e1a28; }
  .editor-layout.dark-ink .title-input::placeholder { color: #604868; }
  .editor-layout.dark-ink .edited-line { color: #604868; }
  .editor-layout.dark-ink .tag-chip { background: rgba(0,0,0,0.08); color: #604868; }
  .editor-layout.dark-ink .tag-input { color: #604868; }

  .editor-layout.light-ink .title-input { color: #ffffff; }
  .editor-layout.light-ink .title-input::placeholder { color: rgba(255,255,255,0.6); }
  .editor-layout.light-ink .edited-line { color: rgba(255,255,255,0.88); }
  .editor-layout.light-ink .tag-chip { background: rgba(255,255,255,0.22); color: #ffffff; }
  .editor-layout.light-ink .tag-input { color: rgba(255,255,255,0.88); }

  @media (max-width: 640px) {
    .editor-header { padding: 0.5rem 0.6rem; }
    .editor-content { padding: 0.9rem 1rem 2rem; }
    .title-input { font-size: 1.3rem; }
  }
</style>
