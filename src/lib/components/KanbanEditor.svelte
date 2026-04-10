<script lang="ts">
  import { moveCard, moveColumn } from "$lib/kanban";
  import type { KanbanColumn, KanbanCard } from "$lib/kanban";

  let { content = $bindable({ columns: [] as KanbanColumn[] }) } = $props<{
    content: { columns: KanbanColumn[] };
  }>();

  function addColumn() {
    content.columns.push({ id: crypto.randomUUID(), name: "New column", cards: [] });
    content = { ...content };
  }
  function removeColumn(id: string) {
    content.columns = content.columns.filter(c => c.id !== id);
  }
  function addCard(col: KanbanColumn) {
    col.cards.push({ id: crypto.randomUUID(), title: "" });
    content = { ...content };
  }
  function removeCard(col: KanbanColumn, cardId: string) {
    col.cards = col.cards.filter(c => c.id !== cardId);
    content = { ...content };
  }

  // ---- Pointer-based drag (works on both mouse and touch) ----

  type DragState =
    | { kind: "card"; card: KanbanCard; fromColId: string }
    | { kind: "col"; colId: string };

  let drag: DragState | null = null;
  let ghostX = $state(0);
  let ghostY = $state(0);
  let ghostText = $state("");
  let isDragging = $state(false);

  function findTarget(x: number, y: number) {
    // Ghost is rendered offset from cursor so it doesn't block elementFromPoint
    let el = document.elementFromPoint(x, y) as HTMLElement | null;
    while (el) {
      const cardId = el.dataset.cardId;
      const colId = el.dataset.colId;
      if (cardId && colId) return { cardId, colId };
      if (colId) return { colId };
      el = el.parentElement;
    }
    return null;
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    ghostX = e.clientX;
    ghostY = e.clientY;
  }

  function onPointerUp(e: PointerEvent) {
    if (!drag) return;
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);

    const target = findTarget(e.clientX, e.clientY);

    if (drag.kind === "card" && target?.colId) {
      const beforeCardId = target.cardId && target.cardId !== drag.card.id
        ? target.cardId
        : null;
      content.columns = moveCard(content.columns, drag.card.id, drag.fromColId, target.colId, beforeCardId);
      content = { ...content };
    } else if (drag.kind === "col" && target?.colId && target.colId !== drag.colId) {
      content.columns = moveColumn(content.columns, drag.colId, target.colId);
      content = { ...content };
    }

    drag = null;
    isDragging = false;
  }

  function startCardDrag(e: PointerEvent, card: KanbanCard, colId: string) {
    // Only start from the handle, not from textarea/buttons
    e.preventDefault();
    drag = { kind: "card", card, fromColId: colId };
    ghostX = e.clientX;
    ghostY = e.clientY;
    ghostText = card.title || "Card";
    isDragging = true;
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }

  function startColDrag(e: PointerEvent, colId: string) {
    e.preventDefault();
    const col = content.columns.find(c => c.id === colId)!;
    drag = { kind: "col", colId };
    ghostX = e.clientX;
    ghostY = e.clientY;
    ghostText = col.name || "Column";
    isDragging = true;
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
  }
</script>

<!-- Ghost follows pointer, offset so it doesn't block elementFromPoint at the cursor tip -->
{#if isDragging}
  <div class="ghost" style="left: {ghostX + 12}px; top: {ghostY + 12}px">
    {ghostText}
  </div>
{/if}

<div class="kanban">
  {#each content.columns as col (col.id)}
    <div class="column" data-col-id={col.id} role="list">
      <div class="col-header" role="presentation">
        <span
          class="handle"
          aria-label="Drag column"
          onpointerdown={(e) => startColDrag(e, col.id)}
        >⠿</span>
        <input
          class="col-name"
          bind:value={col.name}
          onchange={() => content = { ...content }}
        />
        <button class="del" onclick={() => removeColumn(col.id)} title="Remove">×</button>
      </div>

      {#each col.cards as card (card.id)}
        <div
          class="card"
          data-card-id={card.id}
          data-col-id={col.id}
          role="listitem"
        >
          <span
            class="handle"
            aria-label="Drag card"
            onpointerdown={(e) => startCardDrag(e, card, col.id)}
          >⠿</span>
          <textarea
            class="card-text"
            bind:value={card.title}
            placeholder="Card…"
            rows="2"
            onchange={() => content = { ...content }}
          ></textarea>
          <button class="card-del" onclick={() => removeCard(col, card.id)}>×</button>
        </div>
      {/each}

      <button class="add-card" onclick={() => addCard(col)}>+ Add card</button>
    </div>
  {/each}

  <button class="add-col" onclick={addColumn}>+ Add column</button>
</div>

<style>
  .ghost {
    position: fixed; z-index: 1000; pointer-events: none;
    background: var(--accent); color: var(--on-accent);
    padding: 0.45rem 0.85rem; border-radius: var(--radius);
    font-size: 0.85rem; max-width: 200px; font-weight: 600;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    opacity: 0.9;
  }
  .kanban {
    display: flex; gap: 1rem; padding: 1.5rem;
    overflow-x: auto; height: 100%; align-items: flex-start;
    touch-action: pan-y;
  }
  .column {
    width: 260px; flex-shrink: 0;
    background: var(--surface-glass); backdrop-filter: blur(8px); -webkit-backdrop-filter: blur(8px);
    border-radius: var(--radius); border: 1px solid var(--border);
    padding: 0.75rem;
    display: flex; flex-direction: column; gap: 0.4rem;
    box-shadow: 0 4px 16px var(--shadow-color);
  }
  .col-header {
    display: flex; align-items: center; gap: 0.4rem;
    padding-bottom: 0.5rem; margin-bottom: 0.1rem;
    border-bottom: 1px solid var(--border);
  }
  .handle {
    color: var(--muted); font-size: 0.9rem;
    cursor: grab; user-select: none; flex-shrink: 0;
    padding: 2px 4px; touch-action: none;
  }
  .handle:active { cursor: grabbing; }
  .col-name {
    flex: 1; border: none; background: transparent;
    font-weight: 700; font-size: 0.95rem; color: var(--text);
    outline: none; min-width: 0;
  }
  .del { background: none; border: none; cursor: pointer; color: var(--muted); font-size: 1.1rem; line-height: 1; transition: color 0.1s; }
  .del:hover { color: var(--error); }
  .card {
    background: var(--surface); border-radius: 12px;
    border: 1px solid var(--border); padding: 0.5rem;
    display: flex; align-items: flex-start; gap: 0.25rem;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: box-shadow 0.15s ease;
  }
  .card:hover { box-shadow: 0 4px 12px var(--shadow-color-hover); }
  .card-text {
    flex: 1; border: none; background: transparent;
    resize: none; font-size: 0.9rem; color: var(--text);
    outline: none; line-height: 1.5; cursor: text;
  }
  .card-del { background: none; border: none; cursor: pointer; color: var(--muted); transition: color 0.1s; }
  .card-del:hover { color: var(--error); }
  .add-card {
    padding: 0.45rem; border-radius: var(--radius-full);
    border: 1.5px dashed var(--border); background: transparent;
    color: var(--muted); cursor: pointer; width: 100%; font-size: 0.85rem;
    transition: all 0.15s ease;
  }
  .add-card:hover { border-color: var(--accent); color: var(--accent); background: var(--accent-muted); }
  .add-col {
    flex-shrink: 0; width: 200px; padding: 0.6rem; border-radius: var(--radius);
    border: 2px dashed var(--border); background: transparent;
    color: var(--muted); cursor: pointer; font-size: 0.9rem; align-self: flex-start;
    transition: all 0.15s ease;
  }
  .add-col:hover { border-color: var(--accent); color: var(--accent); background: var(--accent-muted); }
</style>
