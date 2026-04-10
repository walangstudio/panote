<script lang="ts">
  import {
    builtinImportParsers,
    makeCustomParser,
    type TableColumn,
    type CustomParserDef,
    type ImportParser,
    type ParseResult,
  } from "$lib/tableParsers";

  interface Props {
    columns: TableColumn[];
    customParsers?: CustomParserDef[];
    onimport: (rows: Record<string, string>[]) => void;
    onclose: () => void;
  }
  let { columns, customParsers, onimport, onclose }: Props = $props();

  type Step = "format" | "input" | "preview";
  let step = $state<Step>("format");
  let selectedParser = $state<ImportParser | null>(null);
  let inputText = $state("");
  let hasHeader = $state(true);
  let parseError = $state("");
  let parsed = $state<ParseResult>({ columns: [], rows: [] });
  let columnMap = $state<Record<string, string>>({});

  // Custom regex state
  let customPattern = $state("");
  let customColumns = $state("");
  let showRegexInput = $state(false);
  let regexError = $state("");

  const allParsers = $derived([
    ...builtinImportParsers,
    ...(customParsers ?? []).map(makeCustomParser),
  ]);

  function pickFormat(parser: ImportParser) {
    selectedParser = parser;
    inputText = "";
    parseError = "";
    if (parser.id === "custom-regex" || parser.id.startsWith("custom-")) {
      // Built-in parsers go straight to input
    }
    step = "input";
  }

  function pickRegex() {
    if (!customPattern.trim() || !customColumns.trim()) return;
    const cols = customColumns.split(",").map((c) => c.trim()).filter(Boolean);
    selectedParser = makeCustomParser({
      id: "custom-adhoc",
      name: "Custom Regex",
      pattern: customPattern,
      columns: cols,
    });
    showRegexInput = false;
    step = "input";
  }

  function runParse() {
    if (!selectedParser || !inputText.trim()) return;
    parseError = "";
    try {
      const options: Record<string, unknown> = {};
      if (selectedParser.id === "csv" || selectedParser.id === "psv") {
        options.hasHeader = hasHeader;
      }
      parsed = selectedParser.parse(inputText, options);
      if (parsed.rows.length === 0) {
        parseError = "No rows parsed from input.";
        return;
      }
      // Auto-map columns by name match
      const map: Record<string, string> = {};
      for (const pc of parsed.columns) {
        const match = columns.find(
          (c) => c.name.toLowerCase() === pc.toLowerCase() || c.id === pc,
        );
        map[pc] = match ? match.id : "";
      }
      columnMap = map;
      step = "preview";
    } catch (e) {
      parseError = String(e);
    }
  }

  function doImport() {
    const mapped = parsed.rows.map((row) => {
      const out: Record<string, string> = {};
      for (const [parsedCol, tableColId] of Object.entries(columnMap)) {
        if (tableColId) out[tableColId] = row[parsedCol] ?? "";
      }
      return out;
    });
    onimport(mapped);
  }

  const previewRows = $derived(parsed.rows.slice(0, 15));
  const mappedCount = $derived(
    Object.values(columnMap).filter((v) => v).length,
  );
</script>

<div class="backdrop" role="presentation" onclick={onclose}></div>
<div class="modal" role="dialog" aria-modal="true">
  <button class="close" onclick={onclose} aria-label="Close">
    <span class="material-symbols-outlined">close</span>
  </button>

  {#if step === "format"}
    <h2>Import Data</h2>
    <p class="subtitle">Choose a format</p>
    <div class="format-grid">
      {#each allParsers as parser}
        <button class="format-card" onclick={() => pickFormat(parser)}>
          <span class="format-icon">
            <span class="material-symbols-outlined">{parser.icon}</span>
          </span>
          <span class="format-name">{parser.name}</span>
          <span class="format-desc">{parser.description}</span>
        </button>
      {/each}
      <button class="format-card" onclick={() => { showRegexInput = true; }}>
        <span class="format-icon regex">
          <span class="material-symbols-outlined">code</span>
        </span>
        <span class="format-name">Custom Regex</span>
        <span class="format-desc">Line-by-line with named groups</span>
      </button>
    </div>

    {#if showRegexInput}
      <div class="regex-setup">
        <label>
          <span class="field-label">Pattern (named groups)</span>
          <input
            class="field-input"
            class:field-error={!!regexError}
            placeholder={'(?<date>\\d{4}-\\d{2}-\\d{2})\\s+(?<event>.+)'}
            bind:value={customPattern}
            oninput={() => {
              if (!customPattern.trim()) { regexError = ""; return; }
              try { new RegExp(customPattern); regexError = ""; }
              catch (e) { regexError = String(e).replace("SyntaxError: ", ""); }
            }}
          />
          {#if regexError}
            <span class="field-error-text">{regexError}</span>
          {/if}
          <span class="field-hint">
            Each line is matched. Use named groups like <code>(?&lt;name&gt;...)</code> for columns.
            <a href="https://developer.mozilla.org/en-US/docs/Web/JavaScript/Guide/Regular_expressions" target="_blank" rel="noopener">Regex reference</a>
          </span>
        </label>
        <label>
          <span class="field-label">Column names (comma-separated)</span>
          <input
            class="field-input"
            placeholder="date, event"
            bind:value={customColumns}
          />
        </label>
        <div class="actions">
          <button class="btn-cancel" onclick={() => showRegexInput = false}>Cancel</button>
          <button class="btn-primary" onclick={pickRegex} disabled={!customPattern.trim() || !customColumns.trim() || !!regexError}>Continue</button>
        </div>
      </div>
    {/if}

  {:else if step === "input"}
    <div class="step-header">
      <button class="back-btn" onclick={() => step = "format"} aria-label="Back">
        <span class="material-symbols-outlined">arrow_back</span>
      </button>
      <h2>{selectedParser?.name}</h2>
    </div>
    <p class="subtitle">Paste your data below</p>

    {#if selectedParser?.id === "csv" || selectedParser?.id === "psv"}
      <label class="toggle-row">
        <input type="checkbox" bind:checked={hasHeader} />
        <span>First row is header</span>
      </label>
    {/if}

    <textarea
      class="data-textarea"
      bind:value={inputText}
      placeholder={selectedParser?.id === "csv" ? "name,age\nAlice,30\nBob,25"
        : selectedParser?.id === "json" ? '[{"name":"Alice","age":"30"}]'
        : selectedParser?.id === "url-desc" ? "my project notes\n~/github.com/user/repo\nmore description"
        : "Paste data here…"}
      rows="10"
    ></textarea>

    {#if parseError}<p class="error">{parseError}</p>{/if}

    <div class="actions">
      <button class="btn-cancel" onclick={() => step = "format"}>Back</button>
      <button class="btn-primary" onclick={runParse} disabled={!inputText.trim()}>Parse</button>
    </div>

  {:else if step === "preview"}
    <div class="step-header">
      <button class="back-btn" onclick={() => step = "input"} aria-label="Back">
        <span class="material-symbols-outlined">arrow_back</span>
      </button>
      <h2>Preview</h2>
      <span class="row-count">{parsed.rows.length} row{parsed.rows.length !== 1 ? "s" : ""}</span>
    </div>

    <div class="mapping-section">
      <p class="mapping-label">Map columns</p>
      <div class="mapping-list">
        {#each parsed.columns as pc}
          <div class="mapping-row">
            <span class="parsed-col">{pc}</span>
            <span class="material-symbols-outlined arrow-icon">arrow_forward</span>
            <select
              class="col-select"
              value={columnMap[pc] ?? ""}
              onchange={(e) => { columnMap[pc] = (e.target as HTMLSelectElement).value; columnMap = { ...columnMap }; }}
            >
              <option value="">Skip</option>
              {#each columns as col}
                <option value={col.id}>{col.name}</option>
              {/each}
            </select>
          </div>
        {/each}
      </div>
    </div>

    <div class="preview-table-wrap">
      <table class="preview-table">
        <thead>
          <tr>
            {#each parsed.columns as pc}
              <th>{pc}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each previewRows as row}
            <tr>
              {#each parsed.columns as pc}
                <td>{row[pc] ?? ""}</td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
      {#if parsed.rows.length > 15}
        <p class="preview-note">Showing 15 of {parsed.rows.length} rows</p>
      {/if}
    </div>

    <div class="actions">
      <button class="btn-cancel" onclick={() => step = "input"}>Back</button>
      <button class="btn-primary" onclick={doImport} disabled={mappedCount === 0}>
        Import {parsed.rows.length} row{parsed.rows.length !== 1 ? "s" : ""}
      </button>
    </div>
  {/if}
</div>

<style>
  .backdrop {
    position: fixed; inset: 0; z-index: 100;
    background: rgba(0, 0, 0, 0.45); backdrop-filter: blur(4px);
  }
  .modal {
    position: fixed; z-index: 101;
    top: 50%; left: 50%; transform: translate(-50%, -50%);
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg); padding: 1.5rem;
    width: min(560px, 95vw);
    max-height: 85vh; overflow-y: auto;
    box-shadow: 0 16px 48px var(--shadow-color-hover);
  }
  .close {
    position: absolute; top: 0.75rem; right: 0.75rem;
    background: var(--accent-muted); border: none; border-radius: var(--radius-full);
    color: var(--muted); cursor: pointer;
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .close:hover { background: var(--accent); color: var(--on-accent); }
  h2 { margin: 0 0 0.25rem; font-size: 1.1rem; font-weight: 700; }
  .subtitle { color: var(--muted); font-size: 0.82rem; margin: 0 0 1rem; }

  /* Step header */
  .step-header { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.25rem; }
  .back-btn {
    background: var(--accent-muted); border: none; border-radius: var(--radius-full);
    color: var(--accent); cursor: pointer;
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .back-btn:hover { background: var(--accent); color: var(--on-accent); }
  .row-count { margin-left: auto; font-size: 0.82rem; color: var(--muted); font-weight: 500; }

  /* Format grid */
  .format-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 0.6rem; }
  .format-card {
    display: flex; flex-direction: column; align-items: center; gap: 0.3rem;
    padding: 0.85rem 0.5rem; border-radius: var(--radius);
    border: 1px solid var(--border); background: var(--surface);
    cursor: pointer; transition: all 0.2s ease;
  }
  .format-card:hover {
    background: var(--hover); box-shadow: 0 4px 16px var(--shadow-color-hover);
    transform: translateY(-2px); border-color: var(--accent-muted);
  }
  .format-card:last-child:nth-child(odd) { grid-column: 1 / -1; }
  .format-icon {
    width: 38px; height: 38px; border-radius: 10px;
    display: flex; align-items: center; justify-content: center;
    background: var(--accent-surface); color: var(--accent);
  }
  .format-icon.regex { background: var(--secondary-surface); color: var(--secondary); }
  .format-name { font-weight: 700; font-size: 0.85rem; }
  .format-desc { font-size: 0.7rem; color: var(--muted); text-align: center; }

  /* Regex setup */
  .regex-setup {
    margin-top: 0.75rem; padding: 0.75rem;
    background: var(--surface-container); border-radius: var(--radius);
    border: 1px solid var(--border);
    display: flex; flex-direction: column; gap: 0.6rem;
  }
  .field-label { font-size: 0.78rem; font-weight: 600; color: var(--text-secondary); display: block; margin-bottom: 0.25rem; }
  .field-input {
    width: 100%; padding: 0.5rem 0.75rem; border: 1px solid var(--border);
    border-radius: var(--radius); background: var(--surface); color: var(--text);
    font-size: 0.85rem; font-family: "JetBrains Mono", monospace; outline: none;
    box-sizing: border-box;
  }
  .field-input:focus { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-muted); }
  .field-input.field-error { border-color: var(--error); }
  .field-input.field-error:focus { box-shadow: 0 0 0 2px color-mix(in srgb, var(--error) 20%, transparent); }
  .field-error-text { font-size: 0.72rem; color: var(--error); margin-top: 0.2rem; display: block; }
  .field-hint { font-size: 0.72rem; color: var(--muted); margin-top: 0.2rem; display: block; line-height: 1.5; }
  .field-hint code { font-family: "JetBrains Mono", monospace; font-size: 0.7rem; background: var(--surface-container); padding: 1px 4px; border-radius: 3px; }
  .field-hint a { color: var(--accent); text-decoration: underline; }

  /* Input step */
  .toggle-row {
    display: flex; align-items: center; gap: 0.5rem;
    font-size: 0.85rem; color: var(--text-secondary); margin-bottom: 0.5rem; cursor: pointer;
  }
  .data-textarea {
    width: 100%; border: 1px solid var(--border); border-radius: var(--radius);
    background: var(--surface-container); color: var(--text);
    padding: 0.75rem; font-size: 0.85rem; line-height: 1.6;
    font-family: "JetBrains Mono", monospace;
    resize: vertical; outline: none; box-sizing: border-box;
  }
  .data-textarea:focus { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-muted); }
  .data-textarea::placeholder { color: var(--muted); }

  /* Preview - mapping */
  .mapping-section { margin: 0.75rem 0; }
  .mapping-label { font-size: 0.82rem; font-weight: 600; color: var(--text-secondary); margin: 0 0 0.5rem; }
  .mapping-list { display: flex; flex-direction: column; gap: 0.4rem; }
  .mapping-row {
    display: flex; align-items: center; gap: 0.5rem;
    padding: 0.35rem 0.5rem;
    background: var(--surface-container); border-radius: var(--radius-sm);
  }
  .parsed-col { font-size: 0.82rem; font-weight: 600; min-width: 80px; color: var(--text); }
  .arrow-icon { font-size: 16px; color: var(--muted); }
  .col-select {
    flex: 1; padding: 0.35rem 0.5rem; border: 1px solid var(--border);
    border-radius: var(--radius-sm); background: var(--surface); color: var(--text);
    font-size: 0.82rem; outline: none;
  }
  .col-select:focus { border-color: var(--accent); }

  /* Preview table */
  .preview-table-wrap {
    max-height: 240px; overflow: auto;
    border: 1px solid var(--border); border-radius: var(--radius);
    margin: 0.75rem 0;
  }
  .preview-table { width: 100%; border-collapse: collapse; font-size: 0.78rem; }
  .preview-table th {
    text-align: left; padding: 0.4rem 0.6rem;
    font-weight: 700; color: var(--text-secondary); text-transform: uppercase;
    letter-spacing: 0.05em; border-bottom: 2px solid var(--border);
    background: var(--surface-container); position: sticky; top: 0;
  }
  .preview-table td {
    padding: 0.3rem 0.6rem; border-bottom: 1px solid var(--border);
    max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .preview-note { text-align: center; font-size: 0.78rem; color: var(--muted); margin: 0.5rem 0 0; }

  /* Actions + buttons */
  .actions { display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 0.75rem; }
  .btn-cancel {
    padding: 0.5rem 1rem; border-radius: var(--radius-full);
    border: 1.5px solid var(--border); background: transparent;
    color: var(--text-secondary); cursor: pointer; font-weight: 500;
    transition: all 0.15s ease;
  }
  .btn-cancel:hover { border-color: var(--text-secondary); }
  .btn-primary {
    padding: 0.5rem 1.25rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; cursor: pointer; transition: transform 0.1s ease;
  }
  .btn-primary:hover { transform: scale(1.03); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; transform: none; }

  .error { color: var(--error); font-size: 0.82rem; margin: 0.5rem 0 0; }

  @media (max-width: 480px) {
    .format-grid { grid-template-columns: 1fr; }
    .mapping-row { flex-wrap: wrap; }
    .parsed-col { min-width: auto; }
  }
</style>
