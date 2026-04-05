<script lang="ts">
  import hljs from "highlight.js";
  import "highlight.js/styles/github-dark.min.css";
  import { tick } from "svelte";

  let { content = $bindable({ lang: "rust", body: "" }) } = $props<{
    content: { lang: string; body: string };
  }>();

  const LANGS = ["rust", "typescript", "javascript", "python", "go", "bash", "sql", "json", "html", "css"];

  let highlighted = $state("");
  let textarea: HTMLTextAreaElement;

  $effect(() => {
    const lang = content.lang;
    const body = content.body;
    tick().then(() => {
      try {
        highlighted = hljs.highlight(body, { language: lang }).value;
      } catch {
        highlighted = hljs.highlightAuto(body).value;
      }
    });
  });

  function syncScroll() {
    const pre = textarea?.parentElement?.querySelector("pre");
    if (pre) {
      pre.scrollTop = textarea.scrollTop;
      pre.scrollLeft = textarea.scrollLeft;
    }
  }
</script>

<div class="code-editor">
  <div class="toolbar">
    <select bind:value={content.lang}>
      {#each LANGS as l}
        <option value={l}>{l}</option>
      {/each}
    </select>
  </div>
  <div class="editor-wrap">
    <pre class="highlighted" aria-hidden="true"><code class="hljs">{@html highlighted}{"\n"}</code></pre>
    <textarea
      bind:this={textarea}
      class="raw"
      placeholder="Paste or write code here…"
      bind:value={content.body}
      spellcheck="false"
      onscroll={syncScroll}
    ></textarea>
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
  .editor-wrap {
    position: relative;
    flex: 1;
    overflow: hidden;
  }
  .raw, .highlighted {
    position: absolute;
    top: 0; left: 0; right: 0; bottom: 0;
    margin: 0; padding: 1rem;
    font-family: "JetBrains Mono", monospace;
    font-size: 0.9rem;
    line-height: 1.6;
    white-space: pre-wrap;
    word-break: break-all;
    overflow: auto;
    border: none;
    background: #1e1e2e;
    tab-size: 2;
  }
  .raw {
    position: absolute;
    z-index: 2;
    color: transparent;
    caret-color: #cdd6f4;
    resize: none;
    outline: none;
    background: transparent;
    -webkit-text-fill-color: transparent;
  }
  .highlighted {
    z-index: 1;
    pointer-events: none;
  }
  .highlighted code {
    background: transparent;
  }
</style>
