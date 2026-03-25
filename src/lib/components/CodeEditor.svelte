<script lang="ts">
  import hljs from "highlight.js";
  import { tick } from "svelte";

  let { content = $bindable({ lang: "rust", body: "" }) } = $props<{
    content: { lang: string; body: string };
  }>();

  const LANGS = ["rust", "typescript", "javascript", "python", "go", "bash", "sql", "json", "html", "css"];

  let highlighted = $state("");

  $effect(async () => {
    const lang = content.lang;
    const body = content.body;
    await tick();
    try {
      highlighted = hljs.highlight(body, { language: lang }).value;
    } catch {
      highlighted = hljs.highlightAuto(body).value;
    }
  });
</script>

<svelte:head>
  <link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/styles/github-dark.min.css" />
</svelte:head>

<div class="code-editor">
  <div class="toolbar">
    <select bind:value={content.lang}>
      {#each LANGS as l}
        <option value={l}>{l}</option>
      {/each}
    </select>
  </div>
  <div class="panes">
    <textarea
      class="raw"
      placeholder="Paste or write code here…"
      bind:value={content.body}
      spellcheck="false"
    ></textarea>
    <pre class="highlighted"><code>{@html highlighted}</code></pre>
  </div>
</div>

<style>
  .code-editor { display: flex; flex-direction: column; height: 100%; }
  .toolbar {
    padding: 0.5rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  select {
    padding: 0.3rem 0.6rem; border-radius: 6px;
    border: 1px solid var(--border);
    background: var(--input-bg); color: var(--text); font-size: 0.85rem;
  }
  .panes { display: flex; flex: 1; overflow: hidden; }
  @media (max-width: 640px) {
    .highlighted { display: none; }
    .raw { border-right: none; }
  }
  .raw, .highlighted {
    flex: 1; margin: 0; padding: 1rem;
    font-family: "JetBrains Mono", monospace; font-size: 0.9rem;
    line-height: 1.6; overflow-y: auto;
  }
  .raw {
    border: none; resize: none; outline: none;
    background: #1e1e2e; color: #cdd6f4;
    border-right: 1px solid var(--border);
  }
  .highlighted { background: #1e1e2e; white-space: pre-wrap; word-break: break-all; }
</style>
