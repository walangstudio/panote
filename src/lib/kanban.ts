export interface KanbanCard { id: string; title: string; note_ref?: string; }
export interface KanbanColumn { id: string; name: string; cards: KanbanCard[]; }

export function moveCard(
  columns: KanbanColumn[],
  cardId: string,
  fromColId: string,
  toColId: string,
  beforeCardId: string | null,
): KanbanColumn[] {
  const cols = columns.map(c => ({ ...c, cards: [...c.cards] }));
  const fromCol = cols.find(c => c.id === fromColId)!;
  const toCol = cols.find(c => c.id === toColId)!;
  const card = fromCol.cards.find(c => c.id === cardId)!;
  fromCol.cards = fromCol.cards.filter(c => c.id !== cardId);
  if (beforeCardId === null) {
    toCol.cards.push(card);
  } else {
    const idx = toCol.cards.findIndex(c => c.id === beforeCardId);
    toCol.cards.splice(idx < 0 ? toCol.cards.length : idx, 0, card);
  }
  return cols;
}

export function moveColumn(
  columns: KanbanColumn[],
  fromColId: string,
  toColId: string,
): KanbanColumn[] {
  const cols = [...columns];
  const from = cols.findIndex(c => c.id === fromColId);
  const [col] = cols.splice(from, 1);
  // re-find target after removal since indices may have shifted
  const to = cols.findIndex(c => c.id === toColId);
  cols.splice(to, 0, col);
  return cols;
}
