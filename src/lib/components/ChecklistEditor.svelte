<script lang="ts">
  interface CheckItem {
    id: string;
    text: string;
    checked: boolean;
    children: CheckItem[];
  }

  let { content = $bindable({ items: [] as CheckItem[] }) } = $props<{ content: { items: CheckItem[] } }>();

  function addItem(list: CheckItem[]) {
    list.push({ id: crypto.randomUUID(), text: "", checked: false, children: [] });
    content = { ...content };
  }

  function removeItem(list: CheckItem[], id: string) {
    const idx = list.findIndex(i => i.id === id);
    if (idx !== -1) list.splice(idx, 1);
    content = { ...content };
  }

  function toggleItem(item: CheckItem) {
    item.checked = !item.checked;
    content = { ...content };
  }
</script>

<div class="checklist">
  {#snippet renderItems(items: CheckItem[], depth: number)}
    <ul style="padding-left: {depth * 1.5}rem">
      {#each items as item (item.id)}
        <li>
          <label class="item {item.checked ? 'done' : ''}">
            <input type="checkbox" checked={item.checked} onchange={() => toggleItem(item)} />
            <input
              class="text-input"
              bind:value={item.text}
              placeholder="Item…"
              onchange={() => content = { ...content }}
            />
            <button class="add-sub" onclick={() => addItem(item.children)} title="Add sub-item">+</button>
            <button class="del" onclick={() => removeItem(items, item.id)} title="Delete">×</button>
          </label>
          {#if item.children.length > 0}
            {@render renderItems(item.children, depth + 1)}
          {/if}
        </li>
      {/each}
    </ul>
  {/snippet}

  {@render renderItems(content.items, 0)}
  <button class="add-btn" onclick={() => addItem(content.items)}>+ Add item</button>
</div>

<style>
  .checklist { padding: 1.5rem; overflow-y: auto; height: 100%; }
  ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.4rem; }
  .item {
    display: flex; align-items: center; gap: 0.5rem;
    padding: 0.4rem 0.6rem; border-radius: 6px;
    background: var(--surface); border: 1px solid var(--border);
  }
  .item.done .text-input { text-decoration: line-through; color: var(--muted); }
  .text-input {
    flex: 1; border: none; background: transparent;
    color: var(--text); font-size: 0.95rem; outline: none;
  }
  .add-sub, .del {
    background: none; border: none; cursor: pointer;
    font-size: 1rem; color: var(--muted); padding: 0 4px;
  }
  .add-sub:hover, .del:hover { color: var(--accent); }
  .add-btn {
    margin-top: 0.75rem; padding: 0.5rem 1rem;
    border-radius: 8px; border: 1px dashed var(--border);
    background: transparent; color: var(--muted); cursor: pointer;
    width: 100%;
  }
  .add-btn:hover { border-color: var(--accent); color: var(--accent); }
</style>
