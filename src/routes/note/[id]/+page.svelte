<script lang="ts">
  import { page } from "$app/state";
  import { onMount } from "svelte";
  import { goto, beforeNavigate } from "$app/navigation";
  import { noteGet, noteCreate, noteUpdate, type NoteDetail, type NoteKind } from "$lib/tauri";
  import { refreshNotes } from "$lib/stores/notes";
  import TransferModal from "$lib/components/TransferModal.svelte";
  import ConfirmModal from "$lib/components/ConfirmModal.svelte";
  import MarkdownEditor from "$lib/components/MarkdownEditor.svelte";
  import ChecklistEditor from "$lib/components/ChecklistEditor.svelte";
  import KanbanEditor from "$lib/components/KanbanEditor.svelte";
  import TableEditor from "$lib/components/TableEditor.svelte";
  import { sidebarOpen } from "$lib/stores/sidebar";
  import { detectFormat } from "$lib/detectFormat";

  const id = $derived(page.params.id);
  const isNew = $derived(id === "new");
  const kindParam = $derived((page.url.searchParams.get("kind") ?? "document") as NoteKind);
  const modeParam = $derived(page.url.searchParams.get("mode"));

  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");

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
    try {
      const note = await noteGet(id);
      kind = note.kind;
      title = note.title;
      content = note.content;
      tags = note.tags;
      showPreview = note.show_preview;
      bgColor = note.bg_color ?? undefined;
      bgImage = note.bg_image ?? undefined;
      savedTitle = title;
      savedContent = JSON.stringify(content);
      savedTags = JSON.stringify(tags);
    } catch (e) {
      error = String(e);
    }
    loading = false;
  });

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
      await refreshNotes();
      justSaved = true;
      goto("/");
    } catch (e) {
      error = String(e);
    }
    saving = false;
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
</script>

{#if loading}
  <div class="loading">Loading…</div>
{:else}
  <div class="editor-layout"
    style:background-color={bgColor}
    style:background-image={bgImage ? `url(${bgImage})` : undefined}
    style:background-size={bgImage ? "cover" : undefined}
    style:background-position={bgImage ? "center" : undefined}
  >
    <header>
      <button class="header-menu-btn" onclick={() => $sidebarOpen = true} aria-label="Open menu">
        <span class="material-symbols-outlined">menu</span>
      </button>
      <a href="/" class="back">
        <span class="material-symbols-outlined">arrow_back</span>
      </a>
      <input class="title-input" placeholder="Title…" bind:value={title} />
      <button class="save-btn" onclick={save} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </button>
      {#if !isNew}
        <div class="menu-wrap">
          <button class="menu-btn" onclick={() => menuOpen = !menuOpen} aria-label="More options">
            <span class="material-symbols-outlined">more_vert</span>
          </button>
          {#if menuOpen}
            <div class="menu-backdrop" role="presentation" onclick={() => menuOpen = false}></div>
            <ul class="menu-dropdown">
              <li><button onclick={() => { menuOpen = false; transferOpen = true; }}>
                <span class="material-symbols-outlined" style="font-size: 18px;">send</span>
                Send note
              </button></li>
            </ul>
          {/if}
        </div>
      {/if}
    </header>

    <div class="meta">
      <div class="tags">
        {#each tags as t}<span class="tag">{t}<button onclick={() => removeTag(t)}>
          <span class="material-symbols-outlined" style="font-size: 14px;">close</span>
        </button></span>{/each}
        <input
          class="tag-input"
          placeholder="Add tags, comma separated…"
          bind:value={tagInput}
          enterkeyhint="done"
          onkeydown={(e) => { if (e.key === "Enter" || e.key === ",") { e.preventDefault(); addTag(); } }}
          oninput={() => { if (tagInput.includes(",")) addTag(); }}
          onblur={addTag}
        />
      </div>
    </div>

    {#if error}<p class="error">{error}</p>{/if}

    {#if transferOpen}
      <TransferModal noteIds={id ? [id] : []} onclose={() => transferOpen = false} />
    {/if}

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

    <div class="editor-footer">
      <label class="preview-toggle">
        <input type="checkbox" bind:checked={showPreview} />
        <span>Show preview on list</span>
      </label>
      <div class="bg-wrap">
        <button class="bg-toggle" onclick={() => bgMenuOpen = !bgMenuOpen} aria-label="Background">
          <span class="material-symbols-outlined" style="font-size: 18px;">palette</span>
        </button>
        {#if bgMenuOpen}
          <div class="bg-backdrop" role="presentation" onclick={() => bgMenuOpen = false}></div>
          <div class="bg-picker">
            <div class="bg-swatches">
              <button class="swatch clear" class:active={!bgColor} onclick={() => { bgColor = undefined; bgImage = undefined; }}
                aria-label="Clear">
                <span class="material-symbols-outlined" style="font-size: 16px;">block</span>
              </button>
              {#each ["#fef3c7","#dcfce7","#dbeafe","#f3e8ff","#fce7f3","#ffedd5","#e0f2fe","#f1f5f9"] as c}
                <button class="swatch" class:active={bgColor === c} style:background-color={c}
                  onclick={() => { bgColor = c; bgImage = undefined; }}></button>
              {/each}
            </div>
            <input class="bg-url-input" type="text" placeholder="Background image URL…"
              value={bgImage ?? ""}
              onchange={(e) => { bgImage = (e.target as HTMLInputElement).value || undefined; }} />
            <div class="bg-upload-row">
              <input type="file" accept="image/png,image/jpeg,image/webp,image/gif"
                class="bg-file-input" id="bg-file-input" onchange={handleBgImageUpload} />
              <label for="bg-file-input" class="bg-upload-btn">
                <span class="material-symbols-outlined" style="font-size: 16px;">upload</span>
                Upload image
              </label>
            </div>
            {#if bgImage}
              <div class="bg-preview-row">
                <span class="bg-preview-thumb" style:background-image="url({bgImage})"></span>
                <button class="bg-clear-btn" onclick={() => { bgImage = undefined; }}>
                  <span class="material-symbols-outlined" style="font-size: 14px;">close</span>
                  Remove
                </button>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
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

<style>
  .loading { display: flex; align-items: center; justify-content: center; height: 100%; color: var(--muted); }
  .editor-layout { display: flex; flex-direction: column; height: 100%; }
  header {
    display: flex; align-items: center; gap: 0.75rem;
    padding: 0.75rem 1.5rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface-glass); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
    box-shadow: 0 2px 12px var(--shadow-color);
  }
  .header-menu-btn {
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    display: flex; align-items: center; padding: 0.25rem; border-radius: var(--radius-full);
    flex-shrink: 0; transition: all 0.15s ease;
  }
  .header-menu-btn:hover { color: var(--accent); background: var(--accent-muted); }
  .back {
    text-decoration: none; color: var(--accent);
    width: 36px; height: 36px; border-radius: var(--radius-full);
    background: var(--accent-muted); display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease; flex-shrink: 0;
  }
  .back:hover { background: var(--accent); color: var(--on-accent); }
  .title-input {
    flex: 1; font-size: 1.2rem; font-weight: 900;
    border: none; background: transparent; color: var(--text); outline: none;
  }
  .title-input::placeholder { color: var(--muted); }
  .save-btn {
    padding: 0.5rem 1.25rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 700; cursor: pointer;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease;
  }
  .save-btn:hover { transform: scale(1.03); }
  .save-btn:disabled { opacity: 0.6; cursor: not-allowed; transform: none; }
  @media (max-width: 640px) {
    header { padding: 0.5rem 0.75rem; gap: 0.5rem; }
    .title-input { font-size: 1rem; min-width: 0; }
    .save-btn { padding: 0.45rem 0.85rem; flex-shrink: 0; }
  }
  .meta {
    display: flex; flex-wrap: wrap; gap: 0.75rem; align-items: center;
    padding: 0.6rem 1.5rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface-container);
  }
  .editor-footer {
    display: flex; align-items: center; gap: 0.75rem;
    padding: 0.5rem 1.5rem;
    border-top: 1px solid var(--border);
    background: var(--surface-container);
    flex-shrink: 0;
  }

  .tags { display: flex; flex-wrap: wrap; gap: 0.4rem; align-items: center; }
  .tag {
    display: inline-flex; align-items: center; gap: 2px;
    background: var(--accent-muted); color: var(--accent);
    border-radius: var(--radius-full); padding: 3px 10px; font-size: 0.78rem; font-weight: 600;
  }
  .tag button {
    background: none; border: none; cursor: pointer; color: inherit; padding: 0;
    display: flex; align-items: center;
  }
  .tag-input {
    border: none; background: transparent; color: var(--text);
    font-size: 0.85rem; outline: none; min-width: 180px; flex: 1;
  }
  .tag-input::placeholder { color: var(--muted); }
  .preview-toggle {
    display: flex; align-items: center; gap: 0.4rem;
    font-size: 0.78rem; color: var(--text-secondary); cursor: pointer; white-space: nowrap;
  }
  .preview-toggle input { accent-color: var(--accent); cursor: pointer; }
  .bg-wrap { position: relative; flex-shrink: 0; }
  .bg-toggle {
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    display: flex; align-items: center; padding: 0.25rem; border-radius: var(--radius-full);
    transition: all 0.15s ease;
  }
  .bg-toggle:hover { color: var(--accent); background: var(--accent-muted); }
  .bg-backdrop {
    position: fixed; inset: 0; z-index: 19;
    background: rgba(0,0,0,0.35); backdrop-filter: blur(2px); -webkit-backdrop-filter: blur(2px);
  }
  .bg-picker {
    position: fixed; inset: 0; z-index: 20;
    margin: auto; width: fit-content; height: fit-content;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 1rem; min-width: 220px;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    display: flex; flex-direction: column; gap: 0.5rem;
  }
  .bg-swatches { display: flex; flex-wrap: wrap; gap: 6px; }
  .swatch {
    width: 28px; height: 28px; border-radius: 50%; border: 2px solid transparent;
    cursor: pointer; transition: all 0.1s ease;
  }
  .swatch.active { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-muted); }
  .swatch.clear {
    background: var(--surface-container); display: flex; align-items: center; justify-content: center;
    color: var(--muted);
  }
  .bg-url-input {
    width: 100%; padding: 0.35rem 0.5rem; border: 1px solid var(--border);
    border-radius: var(--radius-sm); background: var(--surface-container);
    color: var(--text); font-size: 0.78rem; outline: none;
  }
  .bg-url-input:focus { border-color: var(--accent); }
  .bg-upload-row { display: flex; align-items: center; }
  .bg-file-input { display: none; }
  .bg-upload-btn {
    display: flex; align-items: center; gap: 0.4rem;
    padding: 0.3rem 0.6rem; border-radius: var(--radius-sm);
    background: var(--surface-container); border: 1px solid var(--border);
    color: var(--text-secondary); font-size: 0.78rem; font-weight: 500;
    cursor: pointer; transition: all 0.15s ease;
  }
  .bg-upload-btn:hover { border-color: var(--accent); color: var(--accent); }
  .bg-preview-row {
    display: flex; align-items: center; gap: 0.5rem;
  }
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
  .editor-body { flex: 1; overflow: auto; }
  .error { color: var(--error); font-size: 0.85rem; margin: 0.5rem 1.5rem; }
  .menu-wrap { position: relative; flex-shrink: 0; }
  .menu-btn {
    background: var(--accent-muted); border: none; border-radius: var(--radius-full);
    color: var(--accent); cursor: pointer; padding: 0.35rem;
    display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .menu-btn:hover { background: var(--accent); color: var(--on-accent); }
  .menu-backdrop { position: fixed; inset: 0; z-index: 10; }
  .menu-dropdown {
    position: absolute; right: 0; top: calc(100% + 6px); z-index: 11;
    background: var(--surface); border: 1px solid var(--border);
    border-radius: var(--radius); list-style: none; margin: 0; padding: 0.4rem;
    min-width: 160px; box-shadow: 0 8px 32px var(--shadow-color-hover);
  }
  .menu-dropdown li button {
    width: 100%; text-align: left; background: none; border: none;
    padding: 0.55rem 0.85rem; color: var(--text); cursor: pointer; font-size: 0.9rem;
    border-radius: var(--radius-sm); display: flex; align-items: center; gap: 0.5rem;
    transition: background 0.1s ease;
  }
  .menu-dropdown li button:hover { background: var(--hover); }
</style>
