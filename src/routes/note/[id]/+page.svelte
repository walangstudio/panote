<script lang="ts">
  import { page } from "$app/state";
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { noteGet, noteCreate, noteUpdate, type NoteDetail, type NoteKind } from "$lib/tauri";
  import { refreshNotes } from "$lib/stores/notes";
  import TextEditor from "$lib/components/TextEditor.svelte";
  import MarkdownEditor from "$lib/components/MarkdownEditor.svelte";
  import ChecklistEditor from "$lib/components/ChecklistEditor.svelte";
  import CodeEditor from "$lib/components/CodeEditor.svelte";
  import KanbanEditor from "$lib/components/KanbanEditor.svelte";

  const id = $derived(page.params.id);
  const isNew = $derived(id === "new");
  const kindParam = $derived((page.url.searchParams.get("kind") ?? "markdown") as NoteKind);

  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");

  let kind = $state<NoteKind>("markdown");
  let title = $state("");
  let content = $state<unknown>({});
  let tags = $state<string[]>([]);
  let tagInput = $state("");

  onMount(async () => {
    if (isNew) {
      kind = kindParam;
      content = defaultContent(kind);
      loading = false;
      return;
    }
    try {
      const note = await noteGet(id);
      kind = note.kind;
      title = note.title;
      content = note.content;
      tags = note.tags;
    } catch (e) {
      error = String(e);
    }
    loading = false;
  });

  async function save() {
    saving = true;
    error = "";
    try {
      const input = { kind, title, content, tags };
      if (isNew) {
        await noteCreate(input);
      } else {
        await noteUpdate(id, input);
      }
      await refreshNotes();
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

  function defaultContent(k: NoteKind): unknown {
    if (k === "text") return { body: "" };
    if (k === "markdown") return { body: "" };
    if (k === "checklist") return { items: [] };
    if (k === "code") return { lang: "rust", body: "" };
    if (k === "kanban") return { columns: [{ id: crypto.randomUUID(), name: "To do", cards: [] }] };
    return {};
  }
</script>

{#if loading}
  <div class="loading">Loading…</div>
{:else}
  <div class="editor-layout">
    <header>
      <a href="/" class="back">← Notes</a>
      <input class="title-input" placeholder="Title…" bind:value={title} />
      <button class="save-btn" onclick={save} disabled={saving}>
        {saving ? "Saving…" : "Save"}
      </button>
    </header>

    <div class="meta">
      <div class="tags">
        {#each tags as t}<span class="tag">{t}<button onclick={() => removeTag(t)}>×</button></span>{/each}
        <input
          class="tag-input"
          placeholder="Add tags, comma separated…"
          bind:value={tagInput}
          onkeydown={(e) => { if (e.key === "Enter" || e.key === ",") { e.preventDefault(); addTag(); } }}
        />
      </div>
    </div>

    {#if error}<p class="error">{error}</p>{/if}

    <div class="editor-body">
      {#if kind === "text"}
        <TextEditor bind:content />
      {:else if kind === "markdown"}
        <MarkdownEditor bind:content initialPreview={!isNew} />
      {:else if kind === "checklist"}
        <ChecklistEditor bind:content />
      {:else if kind === "code"}
        <CodeEditor bind:content />
      {:else if kind === "kanban"}
        <KanbanEditor bind:content />
      {/if}
    </div>
  </div>
{/if}

<style>
  .loading { display: flex; align-items: center; justify-content: center; height: 100vh; }
  .editor-layout { display: flex; flex-direction: column; height: 100vh; }
  header {
    display: flex; align-items: center; gap: 1rem;
    padding: 0.75rem 1.5rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .back { text-decoration: none; color: var(--accent); font-size: 0.9rem; white-space: nowrap; }
  .title-input {
    flex: 1; font-size: 1.1rem; font-weight: 600;
    border: none; background: transparent; color: var(--text); outline: none;
  }
  .save-btn {
    padding: 0.5rem 1.25rem; border-radius: 7px;
    border: none; background: var(--accent); color: #fff;
    font-weight: 600; cursor: pointer;
  }
  .save-btn:disabled { opacity: 0.6; cursor: not-allowed; }
  @media (max-width: 640px) {
    header { padding: 0.5rem 0.75rem; gap: 0.5rem; }
    .back { font-size: 0.8rem; }
    .title-input { font-size: 1rem; min-width: 0; }
    .save-btn { padding: 0.5rem 0.75rem; flex-shrink: 0; }
  }
  .meta {
    display: flex; flex-wrap: wrap; gap: 1rem; align-items: center;
    padding: 0.6rem 1.5rem;
    border-bottom: 1px solid var(--border);
    background: var(--bg);
  }
  .tags { display: flex; flex-wrap: wrap; gap: 0.4rem; align-items: center; }
  .tag {
    display: inline-flex; align-items: center; gap: 4px;
    background: var(--accent-muted); color: var(--accent);
    border-radius: 12px; padding: 2px 8px; font-size: 0.8rem;
  }
  .tag button { background: none; border: none; cursor: pointer; color: inherit; padding: 0; }
  .tag-input {
    border: none; background: transparent; color: var(--text);
    font-size: 0.85rem; outline: none; min-width: 180px; flex: 1;
  }
  .editor-body { flex: 1; overflow: auto; }
  .error { color: #e74c3c; font-size: 0.85rem; margin: 0.5rem 1.5rem; }
</style>
