import { describe, it, expect } from "vitest";
import { moveCard, moveColumn } from "./kanban";
import type { KanbanColumn } from "./kanban";

function makeCol(id: string, cardIds: string[]): KanbanColumn {
  return { id, name: id, cards: cardIds.map(c => ({ id: c, title: c })) };
}

describe("moveCard", () => {
  it("moves card to another column (append)", () => {
    const cols = [makeCol("a", ["c1", "c2"]), makeCol("b", ["c3"])];
    const result = moveCard(cols, "c1", "a", "b", null);
    expect(result[0].cards.map(c => c.id)).toEqual(["c2"]);
    expect(result[1].cards.map(c => c.id)).toEqual(["c3", "c1"]);
  });

  it("moves card before a specific card in another column", () => {
    const cols = [makeCol("a", ["c1", "c2"]), makeCol("b", ["c3", "c4"])];
    const result = moveCard(cols, "c1", "a", "b", "c3");
    expect(result[0].cards.map(c => c.id)).toEqual(["c2"]);
    expect(result[1].cards.map(c => c.id)).toEqual(["c1", "c3", "c4"]);
  });

  it("reorders card within same column", () => {
    const cols = [makeCol("a", ["c1", "c2", "c3"])];
    const result = moveCard(cols, "c3", "a", "a", "c1");
    expect(result[0].cards.map(c => c.id)).toEqual(["c3", "c1", "c2"]);
  });

  it("does not mutate original columns", () => {
    const cols = [makeCol("a", ["c1"]), makeCol("b", [])];
    moveCard(cols, "c1", "a", "b", null);
    expect(cols[0].cards).toHaveLength(1);
  });
});

describe("moveColumn", () => {
  it("moves column forward", () => {
    const cols = [makeCol("a", []), makeCol("b", []), makeCol("c", [])];
    const result = moveColumn(cols, "a", "c");
    expect(result.map(c => c.id)).toEqual(["b", "a", "c"]);
  });

  it("moves column backward", () => {
    const cols = [makeCol("a", []), makeCol("b", []), makeCol("c", [])];
    const result = moveColumn(cols, "c", "a");
    expect(result.map(c => c.id)).toEqual(["c", "a", "b"]);
  });

  it("does not mutate original array", () => {
    const cols = [makeCol("a", []), makeCol("b", [])];
    moveColumn(cols, "a", "b");
    expect(cols[0].id).toBe("a");
  });
});
