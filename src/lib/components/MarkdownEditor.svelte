<script lang="ts">
  import { marked } from "marked";

  let { content = $bindable({ body: "" }), initialPreview = true } = $props<{ content: { body: string }, initialPreview?: boolean }>();

  let preview = $state(initialPreview);
  let rendered = $derived(marked(content.body ?? ""));
</script>

<div class="md-editor">
  <div class="toolbar">
    <button class={!preview ? "active" : ""} onclick={() => preview = false}>Edit</button>
    <button class={preview ? "active" : ""} onclick={() => preview = true}>Preview</button>
  </div>
  {#if preview}
    <div class="preview" contenteditable="false">{@html rendered}</div>
  {:else}
    <textarea
      class="editor"
      placeholder="Write Markdown here…"
      bind:value={content.body}
    ></textarea>
  {/if}
</div>

<style>
  .md-editor { display: flex; flex-direction: column; height: 100%; }
  .toolbar {
    display: flex; gap: 0.5rem;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  .toolbar button {
    padding: 0.3rem 0.75rem; border-radius: 6px;
    border: 1px solid transparent; background: transparent;
    cursor: pointer; color: var(--text); font-size: 0.85rem;
  }
  .toolbar button.active { background: var(--accent); color: #fff; }
  .editor, .preview {
    flex: 1; padding: 1.5rem; font-size: 1rem;
    line-height: 1.7; color: var(--text);
    overflow-y: auto;
  }
  .editor {
    border: none; resize: none; outline: none;
    background: var(--bg); font-family: "JetBrains Mono", monospace;
  }
  .preview { background: var(--bg); }
</style>
