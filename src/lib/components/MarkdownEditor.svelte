<script lang="ts">
  import { tick } from "svelte";
  import { marked } from "marked";
  import DOMPurify from "dompurify";
  import hljs from "highlight.js";
  import "highlight.js/styles/github-dark.min.css";

  let { content = $bindable({ body: "" }), initialPreview = true } = $props<{ content: { body: string }, initialPreview?: boolean }>();

  let textareaEl: HTMLTextAreaElement | undefined = $state();
  let headingOpen = $state(false);
  let emojiOpen = $state(false);
  let colorOpen = $state(false);

  const fmtActions = [
    { icon: "format_bold", label: "Bold", wrap: "**", placeholder: "bold" },
    { icon: "format_italic", label: "Italic", wrap: "_", placeholder: "italic" },
    { icon: "format_strikethrough", label: "Strikethrough", wrap: "~~", placeholder: "strikethrough" },
    { icon: "format_list_bulleted", label: "Bullet list", prefix: "- ", placeholder: "item" },
    { icon: "format_list_numbered", label: "Numbered list", prefix: "1. ", placeholder: "item" },
    { icon: "code", label: "Inline code", wrap: "`", placeholder: "code" },
    { icon: "data_object", label: "Code block", block: true, placeholder: "code" },
    { icon: "link", label: "Link", link: true, placeholder: "text" },
    { icon: "format_quote", label: "Quote", prefix: "> ", placeholder: "quote" },
    { icon: "horizontal_rule", label: "Horizontal rule", prefix: "\n---\n", placeholder: "" },
  ] as const;

  const headingLevels = [
    { label: "Normal", prefix: "" },
    { label: "Heading 1", prefix: "# " },
    { label: "Heading 2", prefix: "## " },
    { label: "Heading 3", prefix: "### " },
  ];

  const commonEmojis = [
    "😀", "😂", "🥰", "😎", "🤔", "👍", "👎", "❤️", "🔥", "⭐",
    "✅", "❌", "⚠️", "💡", "📌", "📝", "🎉", "🚀", "💻", "📊",
    "🔒", "🔑", "📁", "📎", "🕐", "📅", "✏️", "🗑️", "🔔", "💬",
  ];

  const textColors = [
    { label: "Red", value: "#e53e3e" },
    { label: "Orange", value: "#dd6b20" },
    { label: "Green", value: "#27ae60" },
    { label: "Blue", value: "#3182ce" },
    { label: "Purple", value: "#7c52aa" },
    { label: "Pink", value: "#e040a0" },
  ];

  async function insertMarkdown(action: typeof fmtActions[number]) {
    if (!textareaEl) return;
    const { selectionStart: s, selectionEnd: e } = textareaEl;
    const sel = content.body.slice(s, e) || action.placeholder;
    let insert: string;
    let cursorOffset: number;

    if ("wrap" in action && action.wrap) {
      insert = `${action.wrap}${sel}${action.wrap}`;
      cursorOffset = action.wrap.length;
    } else if ("prefix" in action && action.prefix) {
      insert = `${action.prefix}${sel}`;
      cursorOffset = action.prefix.length;
    } else if ("block" in action) {
      insert = `\n\`\`\`\n${sel}\n\`\`\`\n`;
      cursorOffset = 4;
    } else if ("link" in action) {
      insert = `[${sel}](url)`;
      cursorOffset = 1;
    } else {
      return;
    }

    content.body = content.body.slice(0, s) + insert + content.body.slice(e);
    await tick();
    const newStart = s + cursorOffset;
    const newEnd = newStart + sel.length;
    textareaEl.focus();
    textareaEl.setSelectionRange(newStart, newEnd);
  }

  async function insertHeading(level: typeof headingLevels[number]) {
    if (!textareaEl) return;
    const { selectionStart: s, selectionEnd: e } = textareaEl;
    const sel = content.body.slice(s, e) || "heading";
    const insert = level.prefix ? `${level.prefix}${sel}` : sel;
    content.body = content.body.slice(0, s) + insert + content.body.slice(e);
    await tick();
    const newStart = s + level.prefix.length;
    textareaEl.focus();
    textareaEl.setSelectionRange(newStart, newStart + sel.length);
    headingOpen = false;
  }

  async function insertTable() {
    if (!textareaEl) return;
    const { selectionStart: s } = textareaEl;
    const table = "\n| Column 1 | Column 2 | Column 3 |\n| --- | --- | --- |\n| Cell | Cell | Cell |\n";
    content.body = content.body.slice(0, s) + table + content.body.slice(s);
    await tick();
    textareaEl.focus();
    textareaEl.setSelectionRange(s + 3, s + 11);
  }

  async function insertEmoji(emoji: string) {
    if (!textareaEl) return;
    const { selectionStart: s, selectionEnd: e } = textareaEl;
    content.body = content.body.slice(0, s) + emoji + content.body.slice(e);
    await tick();
    const pos = s + emoji.length;
    textareaEl.focus();
    textareaEl.setSelectionRange(pos, pos);
    emojiOpen = false;
  }

  async function insertColor(color: string) {
    if (!textareaEl) return;
    const { selectionStart: s, selectionEnd: e } = textareaEl;
    const sel = content.body.slice(s, e) || "colored text";
    const open = `<span style="color:${color}">`;
    const insert = `${open}${sel}</span>`;
    content.body = content.body.slice(0, s) + insert + content.body.slice(e);
    await tick();
    const newStart = s + open.length;
    textareaEl.focus();
    textareaEl.setSelectionRange(newStart, newStart + sel.length);
    colorOpen = false;
  }

  const renderer = new marked.Renderer();
  renderer.code = ({ text, lang }: { text: string; lang?: string }) => {
    let highlighted: string;
    if (lang && hljs.getLanguage(lang)) {
      highlighted = hljs.highlight(text, { language: lang }).value;
    } else {
      highlighted = hljs.highlightAuto(text).value;
    }
    return `<pre><code class="hljs">${highlighted}</code></pre>`;
  };
  marked.use({ renderer });

  DOMPurify.addHook("uponSanitizeAttribute", (_node, data) => {
    if (data.attrName === "style") {
      if (!/^color:\s*#[0-9a-fA-F]{3,8}$/.test(data.attrValue.trim())) {
        data.attrValue = "";
      }
    }
  });

  let preview = $state(initialPreview);
  let rendered = $derived(DOMPurify.sanitize(marked(content.body ?? "") as string, { ADD_ATTR: ["style"] }));
</script>

<div class="md-editor">
  <div class="toolbar">
    <button class={!preview ? "active" : ""} onclick={() => preview = false}>Edit</button>
    <button class={preview ? "active" : ""} onclick={() => preview = true}>Preview</button>
  </div>
  {#if !preview}
    <div class="format-bar">
      <!-- Heading dropdown -->
      <div class="dropdown-wrap">
        <button class="fmt-btn" title="Heading" onclick={() => { headingOpen = !headingOpen; emojiOpen = false; colorOpen = false; }}>
          <span class="material-symbols-outlined">title</span>
        </button>
        {#if headingOpen}
          <div class="dropdown-backdrop" role="presentation" onclick={() => headingOpen = false}></div>
          <div class="fmt-dropdown">
            {#each headingLevels as h}
              <button class="fmt-dropdown-item" onclick={() => insertHeading(h)}>{h.label}</button>
            {/each}
          </div>
        {/if}
      </div>

      {#each fmtActions as action}
        <button class="fmt-btn" title={action.label} onclick={() => insertMarkdown(action)}>
          <span class="material-symbols-outlined">{action.icon}</span>
        </button>
      {/each}

      <!-- Table -->
      <button class="fmt-btn" title="Table" onclick={insertTable}>
        <span class="material-symbols-outlined">table_chart</span>
      </button>

      <!-- Emoji picker -->
      <div class="dropdown-wrap">
        <button class="fmt-btn" title="Emoji" onclick={() => { emojiOpen = !emojiOpen; headingOpen = false; colorOpen = false; }}>
          <span class="material-symbols-outlined">emoji_emotions</span>
        </button>
        {#if emojiOpen}
          <div class="dropdown-backdrop" role="presentation" onclick={() => emojiOpen = false}></div>
          <div class="emoji-picker">
            {#each commonEmojis as emoji}
              <button class="emoji-btn" onclick={() => insertEmoji(emoji)}>{emoji}</button>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Color picker -->
      <div class="dropdown-wrap">
        <button class="fmt-btn" title="Text color" onclick={() => { colorOpen = !colorOpen; headingOpen = false; emojiOpen = false; }}>
          <span class="material-symbols-outlined">format_color_text</span>
        </button>
        {#if colorOpen}
          <div class="dropdown-backdrop" role="presentation" onclick={() => colorOpen = false}></div>
          <div class="color-picker">
            {#each textColors as c}
              <button class="color-swatch" style:background-color={c.value} title={c.label}
                onclick={() => insertColor(c.value)}></button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
  {#if preview}
    <div class="preview" contenteditable="false">{@html rendered}</div>
  {:else}
    <textarea
      class="editor"
      placeholder="Write Markdown here…"
      bind:value={content.body}
      bind:this={textareaEl}
    ></textarea>
  {/if}
</div>

<style>
  .md-editor { display: flex; flex-direction: column; height: 100%; }
  .toolbar {
    display: flex; gap: 0.4rem;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface-container);
  }
  .toolbar button {
    padding: 0.35rem 0.85rem; border-radius: var(--radius-full);
    border: none; background: transparent;
    cursor: pointer; color: var(--text-secondary); font-size: 0.85rem; font-weight: 500;
    transition: all 0.15s ease;
  }
  .toolbar button:hover { background: var(--hover); }
  .toolbar button.active { background: var(--accent); color: var(--on-accent); box-shadow: 0 2px 8px var(--shadow-color); }
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
  .preview :global(pre) {
    background: #1e1e2e; border-radius: var(--radius);
    padding: 1rem; overflow-x: auto;
  }
  .preview :global(code.hljs) { background: transparent; }
  .preview :global(code) {
    font-family: "JetBrains Mono", monospace;
    font-size: 0.9rem; line-height: 1.6;
  }
  .preview :global(table) {
    width: 100%; border-collapse: collapse; margin: 1rem 0;
  }
  .preview :global(th), .preview :global(td) {
    border: 1px solid var(--border); padding: 0.5rem 0.75rem; text-align: left;
  }
  .preview :global(th) {
    background: var(--surface-container); font-weight: 600;
  }
  .format-bar {
    display: flex; flex-wrap: wrap; gap: 2px;
    padding: 0.35rem 0.75rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface-container);
    align-items: center;
  }
  .fmt-btn {
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    border: none; background: transparent; border-radius: var(--radius-sm);
    cursor: pointer; color: var(--text-secondary);
    transition: all 0.1s ease;
  }
  .fmt-btn:hover { background: var(--hover); color: var(--text); }
  .fmt-btn .material-symbols-outlined { font-size: 18px; }

  /* Dropdown shared */
  .dropdown-wrap { position: relative; }
  .dropdown-backdrop { position: fixed; inset: 0; z-index: 19; }
  .fmt-dropdown {
    position: absolute; left: 0; top: calc(100% + 4px); z-index: 20;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.25rem; min-width: 130px;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
  }
  .fmt-dropdown-item {
    width: 100%; padding: 0.45rem 0.75rem; border: none; background: none;
    border-radius: var(--radius-sm); cursor: pointer; font-size: 0.85rem;
    color: var(--text); text-align: left;
    transition: background 0.1s ease;
  }
  .fmt-dropdown-item:hover { background: var(--hover); }

  /* Emoji picker */
  .emoji-picker {
    position: absolute; right: 0; top: calc(100% + 4px); z-index: 20;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.5rem;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    display: grid; grid-template-columns: repeat(6, 1fr); gap: 2px;
    width: 220px;
  }
  .emoji-btn {
    width: 32px; height: 32px; border: none; background: none; border-radius: var(--radius-sm);
    cursor: pointer; font-size: 1.1rem; display: flex; align-items: center; justify-content: center;
    transition: background 0.1s ease;
  }
  .emoji-btn:hover { background: var(--hover); }

  /* Color picker */
  .color-picker {
    position: absolute; right: 0; top: calc(100% + 4px); z-index: 20;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.5rem;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    display: flex; gap: 6px;
  }
  .color-swatch {
    width: 24px; height: 24px; border-radius: 50%;
    border: 2px solid transparent; cursor: pointer;
    transition: all 0.1s ease;
  }
  .color-swatch:hover { border-color: var(--text); transform: scale(1.15); }
</style>
