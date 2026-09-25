<script lang="ts">
  import { onMount, onDestroy, untrack } from "svelte";
  import { Editor } from "@tiptap/core";
  import { createExtensions } from "$lib/editorExtensions";
  import { type MarkdownStorage } from "tiptap-markdown";

  // tiptap-markdown adds this at runtime but doesn't augment @tiptap/core's types.
  const readMarkdown = (ed: Editor) =>
    (ed.storage as unknown as { markdown: MarkdownStorage }).markdown.getMarkdown();

  let { content = $bindable({ body: "" }), editable = true }: { content: { body: string }; editable?: boolean } = $props();

  let host: HTMLDivElement | undefined = $state();
  // Deliberately NOT $state: the editor is a mutable third-party instance and
  // making it reactive lets TipTap's own callbacks feed back into effects.
  // `version` is the single reactive signal the toolbar derives from.
  let editor: Editor | undefined;
  let version = $state(0);

  let headingOpen = $state(false);
  let emojiOpen = $state(false);
  let colorOpen = $state(false);

  const headingLevels = [1, 2, 3] as const;

  const textColors = [
    { label: "Red", value: "#e53e3e" },
    { label: "Orange", value: "#dd6b20" },
    { label: "Green", value: "#27ae60" },
    { label: "Blue", value: "#3182ce" },
    { label: "Purple", value: "#7c52aa" },
    { label: "Pink", value: "#e040a0" },
  ];

  const commonEmojis = [
    "😂", "❤️", "🤣", "👍", "😭", "🙏", "😘", "🥰", "😍", "😊",
    "🎉", "🔥", "😁", "💕", "🥺", "😅", "✨", "😆", "👏", "🤦",
    "🙂", "💯", "😢", "💀", "🤞", "👀", "💪", "😏", "🤔", "😩",
  ];

  function closeMenus() {
    headingOpen = false; emojiOpen = false; colorOpen = false;
  }

  onMount(() => {
    editor = new Editor({
      element: host,
      editable,
      // Shared with the round-trip tests so what is tested is what ships.
      // The schema is the security boundary — see $lib/editorExtensions.
      extensions: createExtensions(),
      content: content.body ?? "",
      onUpdate({ editor: ed }) {
        // One-way: editor → content. Never write back into the editor here or
        // the caret jumps to the start on every keystroke.
        content.body = readMarkdown(ed);
        version++;
      },
      onSelectionUpdate() { version++; },
    });
    // Let the toolbar derive its first active-state pass now the editor exists.
    version++;
  });

  onDestroy(() => editor?.destroy());

  // Applied imperatively rather than by reading `editable` inside a tracked
  // effect body: setEditable dispatches a transaction, and tracking whatever
  // that touches is how this turned into an update loop.
  let appliedEditable = true;
  $effect(() => {
    const want = editable;
    untrack(() => {
      if (!editor || appliedEditable === want) return;
      appliedEditable = want;
      editor.setEditable(want);
    });
  });

  // `version` is read so these recompute as the selection moves.
  const isActive = $derived.by(() => {
    version;
    const ed = editor;
    return (name: string, attrs?: Record<string, unknown>) =>
      ed ? ed.isActive(name, attrs) : false;
  });

  type Chain = ReturnType<Editor["chain"]>;

  // Building a chain does nothing until `.run()` executes it.
  function run(fn: (c: Chain) => Chain) {
    if (!editor) return;
    fn(editor.chain().focus()).run();
    closeMenus();
  }

  function setLink() {
    if (!editor) return;
    const prev = editor.getAttributes("link").href ?? "";
    const url = window.prompt("Link URL", prev);
    if (url === null) return;
    if (url === "") {
      editor.chain().focus().extendMarkRange("link").unsetLink().run();
      return;
    }
    editor.chain().focus().extendMarkRange("link").setLink({ href: url }).run();
  }
</script>

<div class="rich-editor">
  {#if editable}
    <div class="format-bar">
      <!-- Headings -->
      <div class="dropdown-wrap">
        <button class="fmt-btn" title="Heading" aria-label="Heading" aria-expanded={headingOpen}
          onclick={() => { const o = !headingOpen; closeMenus(); headingOpen = o; }}>
          <span class="material-symbols-outlined" aria-hidden="true">title</span>
        </button>
        {#if headingOpen}
          <div class="dropdown-backdrop" role="presentation" onclick={closeMenus}></div>
          <div class="fmt-dropdown">
            <button class="fmt-dropdown-item" class:on={isActive("paragraph")} aria-pressed={isActive("paragraph")}
              onclick={() => run(c => c.setParagraph())}>Normal</button>
            {#each headingLevels as level}
              <button class="fmt-dropdown-item" class:on={isActive("heading", { level })} aria-pressed={isActive("heading", { level })}
                onclick={() => run(c => c.toggleHeading({ level }))}>Heading {level}</button>
            {/each}
          </div>
        {/if}
      </div>

      <button class="fmt-btn" class:on={isActive("bold")} aria-pressed={isActive("bold")} title="Bold" aria-label="Bold"
        onclick={() => run(c => c.toggleBold())}>
        <span class="material-symbols-outlined" aria-hidden="true">format_bold</span>
      </button>
      <button class="fmt-btn" class:on={isActive("italic")} aria-pressed={isActive("italic")} title="Italic" aria-label="Italic"
        onclick={() => run(c => c.toggleItalic())}>
        <span class="material-symbols-outlined" aria-hidden="true">format_italic</span>
      </button>
      <button class="fmt-btn" class:on={isActive("strike")} aria-pressed={isActive("strike")} title="Strikethrough" aria-label="Strikethrough"
        onclick={() => run(c => c.toggleStrike())}>
        <span class="material-symbols-outlined" aria-hidden="true">format_strikethrough</span>
      </button>

      <span class="sep"></span>

      <button class="fmt-btn" class:on={isActive("bulletList")} aria-pressed={isActive("bulletList")} title="Bullet list" aria-label="Bullet list"
        onclick={() => run(c => c.toggleBulletList())}>
        <span class="material-symbols-outlined" aria-hidden="true">format_list_bulleted</span>
      </button>
      <button class="fmt-btn" class:on={isActive("orderedList")} aria-pressed={isActive("orderedList")} title="Numbered list" aria-label="Numbered list"
        onclick={() => run(c => c.toggleOrderedList())}>
        <span class="material-symbols-outlined" aria-hidden="true">format_list_numbered</span>
      </button>
      <button class="fmt-btn" class:on={isActive("taskList")} aria-pressed={isActive("taskList")} title="Task list" aria-label="Task list"
        onclick={() => run(c => c.toggleTaskList())}>
        <span class="material-symbols-outlined" aria-hidden="true">checklist</span>
      </button>

      <span class="sep"></span>

      <button class="fmt-btn" class:on={isActive("code")} aria-pressed={isActive("code")} title="Inline code" aria-label="Inline code"
        onclick={() => run(c => c.toggleCode())}>
        <span class="material-symbols-outlined" aria-hidden="true">code</span>
      </button>
      <button class="fmt-btn" class:on={isActive("codeBlock")} aria-pressed={isActive("codeBlock")} title="Code block" aria-label="Code block"
        onclick={() => run(c => c.toggleCodeBlock())}>
        <span class="material-symbols-outlined" aria-hidden="true">data_object</span>
      </button>
      <button class="fmt-btn" class:on={isActive("link")} aria-pressed={isActive("link")} title="Link" aria-label="Link" onclick={setLink}>
        <span class="material-symbols-outlined" aria-hidden="true">link</span>
      </button>
      <button class="fmt-btn" class:on={isActive("blockquote")} aria-pressed={isActive("blockquote")} title="Quote" aria-label="Quote"
        onclick={() => run(c => c.toggleBlockquote())}>
        <span class="material-symbols-outlined" aria-hidden="true">format_quote</span>
      </button>
      <button class="fmt-btn" title="Horizontal rule" aria-label="Horizontal rule"
        onclick={() => run(c => c.setHorizontalRule())}>
        <span class="material-symbols-outlined" aria-hidden="true">horizontal_rule</span>
      </button>

      <span class="sep"></span>

      <!-- Emoji -->
      <div class="dropdown-wrap">
        <button class="fmt-btn" title="Emoji" aria-label="Emoji" aria-expanded={emojiOpen}
          onclick={() => { const o = !emojiOpen; closeMenus(); emojiOpen = o; }}>
          <span class="material-symbols-outlined" aria-hidden="true">emoji_emotions</span>
        </button>
        {#if emojiOpen}
          <div class="dropdown-backdrop" role="presentation" onclick={closeMenus}></div>
          <div class="emoji-grid">
            {#each commonEmojis as e}
              <button class="emoji-btn" onclick={() => run(c => c.insertContent(e))}>{e}</button>
            {/each}
          </div>
        {/if}
      </div>

      <!-- Text colour -->
      <div class="dropdown-wrap">
        <button class="fmt-btn" title="Text color" aria-label="Text color" aria-expanded={colorOpen}
          onclick={() => { const o = !colorOpen; closeMenus(); colorOpen = o; }}>
          <span class="material-symbols-outlined" aria-hidden="true">format_color_text</span>
        </button>
        {#if colorOpen}
          <div class="dropdown-backdrop" role="presentation" onclick={closeMenus}></div>
          <div class="color-picker">
            {#each textColors as c}
              <button class="color-swatch" style:background-color={c.value} title={c.label}
                aria-label={c.label} onclick={() => run(ch => ch.setColor(c.value))}></button>
            {/each}
            <button class="color-clear" title="Clear color" aria-label="Clear color"
              onclick={() => run(c => c.unsetColor())}>
              <span class="material-symbols-outlined" aria-hidden="true">format_color_reset</span>
            </button>
          </div>
        {/if}
      </div>

      <button class="fmt-btn" class:on={isActive("highlight")} aria-pressed={isActive("highlight")} title="Highlight" aria-label="Highlight"
        onclick={() => run(c => c.toggleHighlight({ color: "#ffe58f" }))}>
        <span class="material-symbols-outlined" aria-hidden="true">edit</span>
      </button>
    </div>
  {/if}

  <div class="surface" bind:this={host}></div>
</div>

<style>
  .rich-editor { display: flex; flex-direction: column; height: 100%; min-height: 0; }

  .format-bar {
    display: flex; align-items: center; gap: 2px; flex-wrap: wrap;
    padding: 0.4rem 0.6rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface-container);
    flex-shrink: 0;
  }
  .sep { width: 1px; height: 20px; background: var(--border); margin: 0 4px; }
  .fmt-btn {
    width: 44px; height: 44px; border: none; background: none; border-radius: var(--radius-sm);
    display: flex; align-items: center; justify-content: center;
    cursor: pointer; color: var(--text-secondary);
    transition: background 0.12s ease, color 0.12s ease;
  }
  .fmt-btn:hover { background: var(--hover); color: var(--text); }
  .fmt-btn.on { background: var(--accent-muted); color: var(--accent); }
  .fmt-btn .material-symbols-outlined { font-size: 19px; }

  .dropdown-wrap { position: relative; }
  /* The bar is the fallback containing block for popovers on a narrow screen —
     see the media query at the end. */
  .format-bar { position: relative; }
  .dropdown-backdrop { position: fixed; inset: 0; z-index: 19; }
  .fmt-dropdown, .emoji-grid, .color-picker {
    position: absolute; top: calc(100% + 6px); left: 0; z-index: 20;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.3rem; box-shadow: 0 8px 24px var(--shadow-color-hover);
  }
  .fmt-dropdown { min-width: 150px; }
  .fmt-dropdown-item {
    width: 100%; text-align: left; padding: 0.45rem 0.6rem;
    border: none; background: none; border-radius: var(--radius-sm);
    cursor: pointer; font-size: 0.85rem; color: var(--text-secondary); font-family: inherit;
  }
  .fmt-dropdown-item:hover { background: var(--hover); color: var(--text); }
  .fmt-dropdown-item.on { color: var(--accent); font-weight: 700; }

  .emoji-grid { display: grid; grid-template-columns: repeat(6, 1fr); gap: 2px; width: 220px; }
  .emoji-btn {
    border: none; background: none; cursor: pointer; font-size: 1.1rem;
    padding: 4px; border-radius: var(--radius-sm);
  }
  .emoji-btn:hover { background: var(--hover); }

  .color-picker { display: flex; gap: 5px; align-items: center; }
  .color-swatch {
    width: 22px; height: 22px; border-radius: 50%; cursor: pointer;
    border: 2px solid transparent; transition: transform 0.12s ease, border-color 0.12s ease;
  }
  .color-swatch:hover { border-color: var(--text); transform: scale(1.15); }

  /* A popover is anchored to its own toolbar button, so one near the right edge
     opened partly off screen — the colour row and the 220px emoji grid both did.
     Narrow screens anchor to the bar instead and span it, which cannot overflow
     whichever button was pressed. */
  @media (max-width: 640px) {
    .dropdown-wrap { position: static; }
    .fmt-dropdown, .emoji-grid, .color-picker {
      left: 0.4rem; right: 0.4rem; width: auto; min-width: 0;
      max-height: 50vh; overflow-y: auto;
    }
    .emoji-grid { grid-template-columns: repeat(8, 1fr); }
    .color-picker { flex-wrap: wrap; row-gap: 8px; }
  }
  .color-clear {
    width: 22px; height: 22px; border: none; background: none; cursor: pointer;
    color: var(--muted); display: flex; align-items: center; justify-content: center;
  }
  .color-clear:hover { color: var(--accent); }
  .color-clear .material-symbols-outlined { font-size: 17px; }

  /* The editing surface itself */
  .surface { flex: 1; min-height: 0; overflow-y: auto; background: var(--bg); }

  .surface :global(.tiptap) {
    /* The editor container already pads; keep this tight so the two don't stack
       into a wide dead margin around the text. */
    outline: none; padding: 0.7rem 0.9rem; min-height: 100%;
    font-size: 1rem; line-height: 1.7; color: var(--text);
  }
  .surface :global(.tiptap > * + *) { margin-top: 0.7em; }
  .surface :global(.tiptap p.is-editor-empty:first-child::before) {
    content: attr(data-placeholder); float: left; height: 0;
    color: var(--muted); pointer-events: none;
  }
  .surface :global(.tiptap h1) { font-size: 1.6rem; font-weight: 900; line-height: 1.25; }
  .surface :global(.tiptap h2) { font-size: 1.3rem; font-weight: 700; line-height: 1.3; }
  .surface :global(.tiptap h3) { font-size: 1.1rem; font-weight: 700; }
  .surface :global(.tiptap ul), .surface :global(.tiptap ol) { padding-left: 1.4rem; }
  .surface :global(.tiptap blockquote) {
    border-left: 3px solid var(--accent); padding-left: 0.9rem; color: var(--text-secondary);
  }
  .surface :global(.tiptap code) {
    background: var(--surface-container); border-radius: 4px;
    padding: 0.1em 0.35em; font-size: 0.9em;
  }
  .surface :global(.tiptap pre) {
    background: #1e1e2e; color: #e6e0ec; border-radius: var(--radius);
    padding: 0.9rem 1rem; overflow-x: auto;
  }
  .surface :global(.tiptap pre code) { background: none; padding: 0; font-size: 0.88rem; }
  .surface :global(.tiptap hr) { border: none; border-top: 1px solid var(--border); }
  .surface :global(.tiptap a) { color: var(--accent); text-decoration: underline; }
  .surface :global(.tiptap mark) { background: #ffe58f; border-radius: 3px; padding: 0 2px; }

  /* Task list checkboxes */
  .surface :global(.tiptap ul[data-type="taskList"]) { list-style: none; padding-left: 0.2rem; }
  .surface :global(.tiptap ul[data-type="taskList"] li) { display: flex; gap: 0.5rem; align-items: flex-start; }
  .surface :global(.tiptap ul[data-type="taskList"] li > label) { margin-top: 0.25em; }
  .surface :global(.tiptap ul[data-type="taskList"] li > div) { flex: 1; min-width: 0; }

  /* lowlight / highlight.js token colours, scoped to code blocks */
  .surface :global(.hljs-comment), .surface :global(.hljs-quote) { color: #7f849c; font-style: italic; }
  .surface :global(.hljs-keyword), .surface :global(.hljs-selector-tag) { color: #cba6f7; }
  .surface :global(.hljs-string), .surface :global(.hljs-attr) { color: #a6e3a1; }
  .surface :global(.hljs-number), .surface :global(.hljs-literal) { color: #fab387; }
  .surface :global(.hljs-title), .surface :global(.hljs-name) { color: #89b4fa; }
  .surface :global(.hljs-type), .surface :global(.hljs-built_in) { color: #f9e2af; }
  .surface :global(.hljs-variable), .surface :global(.hljs-template-variable) { color: #f38ba8; }
</style>
