<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import {
    slugifyColumn,
    type TableContent,
    type TableColumn,
    type TableRow,
  } from "$lib/tableParsers";
  import TableImportModal from "$lib/components/TableImportModal.svelte";
  import ConfirmModal from "$lib/components/ConfirmModal.svelte";

  let { content = $bindable({ columns: [], rows: [] }) } = $props<{
    content: TableContent;
  }>();

  let showColumnSetup = $state(false);
  let showAddRow = $state(false);
  let showImport = $state(false);
  let rowToDelete = $state<string | null>(null);

  let activeRowId = $state<string | null>(null);
  let rowModalMode = $state<"view" | "edit">("view");
  let editCells = $state<Record<string, string>>({});

  // Column setup state
  let setupNames = $state<string[]>([]);

  // Add row form state
  let newRowCells = $state<Record<string, string>>({});

  function isLongText(v: string): boolean {
    return v.length > 60 || v.includes("\n");
  }

  onMount(() => {
    if (content.columns.length === 0) openColumnSetup();
  });

  // ---- Column Setup ----

  function openColumnSetup() {
    setupNames = content.columns.length > 0
      ? content.columns.map((c: TableColumn) => c.name)
      : [""];
    showColumnSetup = true;
  }

  function addSetupColumn() {
    setupNames = [...setupNames, ""];
  }

  function removeSetupColumn(i: number) {
    setupNames = setupNames.filter((_, idx) => idx !== i);
  }

  function moveColumn(i: number, dir: -1 | 1) {
    const j = i + dir;
    if (j < 0 || j >= setupNames.length) return;
    const arr = [...setupNames];
    [arr[i], arr[j]] = [arr[j], arr[i]];
    setupNames = arr;
  }

  function finishColumnSetup() {
    const names = setupNames.map((n) => n.trim()).filter(Boolean);
    if (names.length === 0) return;
    const ids: string[] = [];
    const newCols: TableColumn[] = names.map((name) => {
      const id = slugifyColumn(name, ids);
      ids.push(id);
      return { id, name };
    });

    // Preserve existing row data for columns that still exist
    const oldIdMap = new Map(content.columns.map((c: TableColumn) => [c.name.toLowerCase(), c.id]));
    const rows = content.rows.map((row: TableRow) => {
      const cells: Record<string, string> = {};
      for (const col of newCols) {
        const oldId = oldIdMap.get(col.name.toLowerCase());
        cells[col.id] = (oldId ? row.cells[oldId] : row.cells[col.id]) ?? "";
      }
      return { ...row, cells };
    });

    content = { ...content, columns: newCols, rows };
    showColumnSetup = false;
  }

  const canFinishSetup = $derived(
    setupNames.some((n) => n.trim().length > 0),
  );

  // ---- Add Row ----

  function openAddRow() {
    newRowCells = {};
    for (const col of content.columns) newRowCells[col.id] = "";
    showAddRow = true;
  }

  function saveRow() {
    const row: TableRow = { id: crypto.randomUUID(), cells: { ...newRowCells } };
    content.rows = [...content.rows, row];
    content = { ...content };
    // Clear for next entry
    for (const col of content.columns) newRowCells[col.id] = "";
    newRowCells = { ...newRowCells };
  }

  function handleRowKeydown(e: KeyboardEvent, isLast: boolean) {
    if (e.key === "Enter" && isLast) {
      e.preventDefault();
      saveRow();
    }
  }

  // ---- Import ----

  function handleImport(rows: Record<string, string>[]) {
    const newRows: TableRow[] = rows.map((cells) => ({
      id: crypto.randomUUID(),
      cells,
    }));
    content.rows = [...content.rows, ...newRows];
    content = { ...content };
    showImport = false;
  }

  // ---- Row detail modal ----

  function openRow(id: string) {
    const row = content.rows.find((r: TableRow) => r.id === id);
    if (!row) return;
    activeRowId = id;
    rowModalMode = "view";
    editCells = { ...row.cells };
  }

  function closeRowModal() {
    activeRowId = null;
    rowModalMode = "view";
  }

  function enterEditMode() {
    const row = content.rows.find((r: TableRow) => r.id === activeRowId);
    if (!row) return;
    editCells = { ...row.cells };
    rowModalMode = "edit";
  }

  function cancelEdit() {
    const row = content.rows.find((r: TableRow) => r.id === activeRowId);
    if (row) editCells = { ...row.cells };
    rowModalMode = "view";
  }

  function saveEdit() {
    const row = content.rows.find((r: TableRow) => r.id === activeRowId);
    if (!row) return;
    row.cells = { ...editCells };
    content = { ...content };
    rowModalMode = "view";
  }

  function handleRowKey(e: KeyboardEvent, id: string) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      openRow(id);
    }
  }

  function confirmRemoveRow() {
    if (!rowToDelete) return;
    const id = rowToDelete;
    rowToDelete = null;
    content.rows = content.rows.filter((r: TableRow) => r.id !== id);
    content = { ...content };
    if (activeRowId === id) closeRowModal();
  }

  function isUrl(value: string): boolean {
    return /^https?:\/\//.test(value);
  }
</script>

{#if content.columns.length > 0}
  <div class="table-editor">
    <div class="toolbar">
      <div class="toolbar-actions">
        <button class="tool-btn" onclick={openAddRow} title="Add Row">
          <span class="material-symbols-outlined" style="font-size: 18px;">add</span>
          Add Row
        </button>
        <button class="tool-btn" onclick={() => showImport = true} title="Import">
          <span class="material-symbols-outlined" style="font-size: 18px;">upload</span>
          Import
        </button>
        <button class="tool-btn secondary" onclick={openColumnSetup} title="Edit Columns">
          <span class="material-symbols-outlined" style="font-size: 18px;">view_column</span>
        </button>
      </div>
    </div>

    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            {#each content.columns as col}
              <th>{col.name}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each content.rows as row (row.id)}
            <tr
              class="row-clickable"
              role="button"
              tabindex="0"
              onclick={() => openRow(row.id)}
              onkeydown={(e) => handleRowKey(e, row.id)}
            >
              {#each content.columns as col}
                {@const val = row.cells[col.id] ?? ""}
                <td>
                  {#if isUrl(val)}
                    <div class="url-cell">
                      <a
                        href={val}
                        target="_blank"
                        rel="noopener"
                        class="url-link"
                        onclick={(e) => e.stopPropagation()}
                      >
                        <span class="material-symbols-outlined" style="font-size: 14px;">open_in_new</span>
                      </a>
                      <span class="cell-text">{val}</span>
                    </div>
                  {:else}
                    <span class="cell-text">{val}</span>
                  {/if}
                </td>
              {/each}
            </tr>
          {/each}
          {#if content.rows.length === 0}
            <tr>
              <td colspan={content.columns.length} class="empty-row">
                No rows yet. Click "Add Row" or "Import" to get started.
              </td>
            </tr>
          {/if}
        </tbody>
      </table>
    </div>
  </div>
{/if}

<!-- Column Setup Modal -->
{#if showColumnSetup}
  <div class="backdrop" role="presentation" onclick={() => { if (content.columns.length > 0) showColumnSetup = false; else goto("/"); }}></div>
  <div class="modal" role="dialog" aria-modal="true">
    <button class="modal-close" onclick={() => { if (content.columns.length > 0) showColumnSetup = false; else goto("/"); }} aria-label="Close">
      <span class="material-symbols-outlined">close</span>
    </button>
    <h2>Define Columns</h2>
    <p class="modal-subtitle">Set up the columns for your table</p>

    <div class="col-list">
      {#each setupNames as name, i}
        <div class="col-entry">
          <div class="col-reorder">
            <button
              class="reorder-btn"
              disabled={i === 0}
              onclick={() => moveColumn(i, -1)}
              aria-label="Move up"
            >
              <span class="material-symbols-outlined" style="font-size: 16px;">keyboard_arrow_up</span>
            </button>
            <button
              class="reorder-btn"
              disabled={i === setupNames.length - 1}
              onclick={() => moveColumn(i, 1)}
              aria-label="Move down"
            >
              <span class="material-symbols-outlined" style="font-size: 16px;">keyboard_arrow_down</span>
            </button>
          </div>
          <input
            class="col-name-input"
            placeholder="Column name"
            bind:value={setupNames[i]}
            onkeydown={(e) => { if (e.key === "Enter") { e.preventDefault(); addSetupColumn(); } }}
          />
          <button
            class="col-del"
            onclick={() => removeSetupColumn(i)}
            disabled={setupNames.length <= 1}
            aria-label="Remove column"
          >
            <span class="material-symbols-outlined" style="font-size: 16px;">close</span>
          </button>
        </div>
      {/each}
    </div>

    <button class="add-col-btn" onclick={addSetupColumn}>
      <span class="material-symbols-outlined" style="font-size: 18px;">add</span>
      Add Column
    </button>

    <div class="modal-actions">
      <button class="btn-cancel" onclick={() => { if (content.columns.length > 0) showColumnSetup = false; else goto("/"); }}>Cancel</button>
      <button class="btn-primary" onclick={finishColumnSetup} disabled={!canFinishSetup}>Done</button>
    </div>
  </div>
{/if}

<!-- Add Row Modal -->
{#if showAddRow}
  <div class="backdrop" role="presentation" onclick={() => showAddRow = false}></div>
  <div class="modal" role="dialog" aria-modal="true">
    <button class="modal-close" onclick={() => showAddRow = false} aria-label="Close">
      <span class="material-symbols-outlined">close</span>
    </button>
    <h2>Add Row</h2>
    <p class="modal-subtitle">Fill in the values</p>

    <div class="row-form">
      {#each content.columns as col, i}
        <label class="form-field">
          <span class="form-label">{col.name}</span>
          {#if isLongText(newRowCells[col.id] ?? "")}
            <textarea
              class="form-input"
              rows="4"
              bind:value={newRowCells[col.id]}
              placeholder={col.name}
            ></textarea>
          {:else}
            <input
              class="form-input"
              bind:value={newRowCells[col.id]}
              placeholder={col.name}
              onkeydown={(e) => handleRowKeydown(e, i === content.columns.length - 1)}
            />
          {/if}
        </label>
      {/each}
    </div>

    <div class="modal-actions">
      <button class="btn-cancel" onclick={() => showAddRow = false}>Close</button>
      <button class="btn-primary" onclick={saveRow}>Save</button>
    </div>
  </div>
{/if}

<!-- Row Detail Modal -->
{#if activeRowId}
  {@const row = content.rows.find((r: TableRow) => r.id === activeRowId)}
  {#if row}
    <div class="backdrop" role="presentation" onclick={closeRowModal}></div>
    <div class="modal" role="dialog" aria-modal="true">
      <button class="modal-close" onclick={closeRowModal} aria-label="Close">
        <span class="material-symbols-outlined">close</span>
      </button>
      <h2>{rowModalMode === "edit" ? "Edit Row" : "Row Details"}</h2>
      <p class="modal-subtitle">
        {rowModalMode === "edit" ? "Update field values" : "Tap Edit to change values"}
      </p>

      <div class="row-form">
        {#each content.columns as col}
          {@const viewVal = row.cells[col.id] ?? ""}
          {@const editVal = editCells[col.id] ?? ""}
          <label class="form-field">
            <span class="form-label">{col.name}</span>
            {#if rowModalMode === "view"}
              {#if isLongText(viewVal)}
                <div class="view-value view-long">{viewVal || "—"}</div>
              {:else if isUrl(viewVal)}
                <a class="view-value view-url" href={viewVal} target="_blank" rel="noopener">{viewVal}</a>
              {:else}
                <div class="view-value">{viewVal || "—"}</div>
              {/if}
            {:else if isLongText(editVal)}
              <textarea class="form-input" rows="4" bind:value={editCells[col.id]}></textarea>
            {:else}
              <input class="form-input" bind:value={editCells[col.id]} />
            {/if}
          </label>
        {/each}
      </div>

      <div class="modal-actions">
        {#if rowModalMode === "view"}
          <button class="btn-cancel btn-destructive" onclick={() => { rowToDelete = activeRowId; }}>
            Delete
          </button>
          <button class="btn-primary" onclick={enterEditMode}>Edit</button>
        {:else}
          <button class="btn-cancel" onclick={cancelEdit}>Cancel</button>
          <button class="btn-primary" onclick={saveEdit}>Save</button>
        {/if}
      </div>
    </div>
  {/if}
{/if}

<!-- Import Modal -->
{#if showImport}
  <TableImportModal
    columns={content.columns}
    customParsers={content.customParsers}
    onimport={handleImport}
    onclose={() => showImport = false}
  />
{/if}

{#if rowToDelete}
  <ConfirmModal
    title="Delete row?"
    message="This row will be removed from the table."
    confirmLabel="Delete"
    destructive
    onconfirm={confirmRemoveRow}
    oncancel={() => rowToDelete = null}
  />
{/if}

<style>
  /* Table editor layout */
  .table-editor { display: flex; flex-direction: column; height: 100%; }
  .toolbar {
    display: flex; align-items: center; justify-content: flex-end;
    padding: 0.5rem 1rem;
    border-bottom: 1px solid var(--border);
    background: var(--surface-container);
  }
  .toolbar-actions { display: flex; gap: 0.4rem; }
  .tool-btn {
    display: flex; align-items: center; gap: 0.3rem;
    padding: 0.4rem 0.75rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; font-size: 0.8rem; cursor: pointer;
    transition: transform 0.1s ease;
  }
  .tool-btn:hover { transform: scale(1.03); }
  .tool-btn.secondary {
    background: var(--accent-muted); color: var(--accent);
    padding: 0.4rem;
  }
  .tool-btn.secondary:hover { background: var(--accent); color: var(--on-accent); }

  /* Table */
  .table-wrap { flex: 1; overflow: auto; padding: 1rem; }
  table { width: 100%; border-collapse: collapse; }
  th {
    text-align: left; padding: 0.5rem 0.75rem;
    font-size: 0.78rem; font-weight: 700; color: var(--text-secondary);
    text-transform: uppercase; letter-spacing: 0.05em;
    border-bottom: 2px solid var(--border);
    background: var(--surface-container);
    position: sticky; top: 0; z-index: 1;
  }
  td {
    padding: 0.6rem 0.75rem;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
    max-width: 220px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    font-size: 0.88rem;
    color: var(--text);
  }
  tbody tr.row-clickable { cursor: pointer; transition: background 0.1s ease; }
  tbody tr.row-clickable:hover { background: var(--hover); }
  tbody tr.row-clickable:focus { outline: 2px solid var(--accent); outline-offset: -2px; }
  .cell-text { display: inline-block; max-width: 100%; overflow: hidden; text-overflow: ellipsis; vertical-align: middle; }
  .url-cell { display: flex; align-items: center; gap: 0.3rem; min-width: 0; }
  .url-cell .cell-text { flex: 1; min-width: 0; }
  .url-link {
    flex-shrink: 0; display: flex; align-items: center;
    color: var(--accent); text-decoration: none;
    padding: 2px; border-radius: var(--radius-sm);
    transition: background 0.1s ease;
  }
  .url-link:hover { background: var(--accent-muted); }
  .view-value {
    padding: 0.55rem 0.75rem; border-radius: var(--radius);
    background: var(--surface-container); color: var(--text);
    font-size: 0.9rem; word-break: break-word;
  }
  .view-long { white-space: pre-wrap; min-height: 4.5rem; }
  .view-url { color: var(--accent); text-decoration: none; display: block; }
  .view-url:hover { text-decoration: underline; }
  .btn-destructive { border-color: var(--error); color: var(--error); }
  .btn-destructive:hover { background: var(--error); color: var(--on-accent); border-color: var(--error); }
  .empty-row {
    text-align: center; color: var(--muted); padding: 2rem !important;
    font-size: 0.88rem;
  }

  /* Shared modal styles */
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
    width: min(420px, 92vw);
    max-height: 85vh; overflow-y: auto;
    box-shadow: 0 16px 48px var(--shadow-color-hover);
  }
  .modal-close {
    position: absolute; top: 0.75rem; right: 0.75rem;
    background: var(--accent-muted); border: none; border-radius: var(--radius-full);
    color: var(--muted); cursor: pointer;
    width: 32px; height: 32px; display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .modal-close:hover { background: var(--accent); color: var(--on-accent); }
  .modal h2 { margin: 0 0 0.25rem; font-size: 1.1rem; font-weight: 700; }
  .modal-subtitle { color: var(--muted); font-size: 0.82rem; margin: 0 0 1rem; }
  .modal-actions { display: flex; justify-content: flex-end; gap: 0.5rem; margin-top: 1rem; }

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

  /* Column setup */
  .col-list { display: flex; flex-direction: column; gap: 0.5rem; }
  .col-entry { display: flex; align-items: center; gap: 0.4rem; }
  .col-reorder { display: flex; flex-direction: column; }
  .reorder-btn {
    background: none; border: none; cursor: pointer; color: var(--muted);
    padding: 0; display: flex; align-items: center; line-height: 1;
    transition: color 0.1s ease;
  }
  .reorder-btn:hover:not(:disabled) { color: var(--accent); }
  .reorder-btn:disabled { opacity: 0.3; cursor: default; }
  .col-name-input {
    flex: 1; padding: 0.5rem 0.75rem; border: 1px solid var(--border);
    border-radius: var(--radius); background: var(--surface); color: var(--text);
    font-size: 0.9rem; outline: none;
  }
  .col-name-input:focus { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-muted); }
  .col-name-input::placeholder { color: var(--muted); }
  .col-del {
    background: none; border: none; cursor: pointer; color: var(--muted);
    padding: 4px; border-radius: var(--radius-full);
    display: flex; align-items: center; transition: all 0.1s ease;
  }
  .col-del:hover:not(:disabled) { color: var(--error); }
  .col-del:disabled { opacity: 0.3; cursor: default; }
  .add-col-btn {
    display: flex; align-items: center; gap: 0.3rem; justify-content: center;
    margin-top: 0.5rem; padding: 0.5rem;
    border-radius: var(--radius-full); border: 1.5px dashed var(--border);
    background: transparent; color: var(--muted); cursor: pointer;
    font-size: 0.85rem; font-weight: 500; width: 100%;
    transition: all 0.15s ease;
  }
  .add-col-btn:hover { border-color: var(--accent); color: var(--accent); background: var(--accent-muted); }

  /* Add row form */
  .row-form {
    display: flex; flex-direction: column; gap: 0.6rem;
    max-height: 60vh; overflow-y: auto;
  }
  .form-field { display: flex; flex-direction: column; gap: 0.2rem; }
  .form-label { font-size: 0.78rem; font-weight: 600; color: var(--text-secondary); }
  .form-input {
    padding: 0.5rem 0.75rem; border: 1px solid var(--border);
    border-radius: var(--radius); background: var(--surface); color: var(--text);
    font-size: 0.9rem; outline: none; font-family: inherit;
    width: 100%; box-sizing: border-box; resize: vertical;
  }
  .form-input:focus { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-muted); }
  .form-input::placeholder { color: var(--muted); }

  @media (max-width: 640px) {
    .toolbar { padding: 0.4rem 0.5rem; }
    .tool-btn { padding: 0.35rem 0.6rem; font-size: 0.75rem; }
  }
</style>
