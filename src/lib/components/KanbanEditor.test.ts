// @vitest-environment happy-dom
//
// The board reorders by pointer drag rather than HTML5 DnD, so it works on
// touch. That means the drop target is resolved with elementFromPoint against
// data-col-id / data-card-id, and a wrong resolution silently moves a card to
// the wrong column.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import type { KanbanColumn } from "$lib/kanban";
import KanbanEditor from "./KanbanEditor.svelte";

const col = (id: string, name: string, cards: { id: string; title: string }[] = []): KanbanColumn =>
  ({ id, name, cards }) as KanbanColumn;

let cleanup: (() => void) | null = null;
const flush = () => new Promise(r => setTimeout(r, 0));

function setup(columns: KanbanColumn[] = []) {
  const content = { columns };
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(KanbanEditor, { target, props: { content } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target, content };
}

const colNames = (t: HTMLElement) =>
  [...t.querySelectorAll<HTMLInputElement>(".col-name")].map(i => i.value);
const cardTitles = (t: HTMLElement, colId: string) =>
  [...t.querySelectorAll<HTMLElement>(`.card[data-col-id="${colId}"]`)]
    .map(c => c.querySelector("textarea")!.value);

/// Drive one full pointer drag. `dropOn` is the element elementFromPoint should
/// report under the cursor when the button is released.
async function drag(handle: Element, dropOn: Element | null) {
  handle.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 5, clientY: 5 }));
  await flush();
  vi.spyOn(document, "elementFromPoint").mockReturnValue(dropOn as Element);
  window.dispatchEvent(new PointerEvent("pointermove", { clientX: 50, clientY: 60 }));
  window.dispatchEvent(new PointerEvent("pointerup", { clientX: 50, clientY: 60 }));
  await flush();
}

beforeEach(() => {
  vi.restoreAllMocks();
});
afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

describe("columns", () => {
  it("starts empty and adds a named column", async () => {
    const { target, content } = setup();
    expect(colNames(target)).toEqual([]);

    target.querySelector<HTMLButtonElement>(".add-col")!.click();
    await flush();

    expect(content.columns).toHaveLength(1);
    expect(colNames(target)).toEqual(["New column"]);
  });

  it("appends further columns to the right", async () => {
    const { target } = setup([col("a", "To do")]);
    target.querySelector<HTMLButtonElement>(".add-col")!.click();
    await flush();
    expect(colNames(target)).toEqual(["To do", "New column"]);
  });

  it("removes the column whose × was pressed", async () => {
    const { target, content } = setup([col("a", "To do"), col("b", "Doing"), col("c", "Done")]);
    target.querySelectorAll<HTMLButtonElement>(".col-header .del")[1].click();
    await flush();
    expect(content.columns.map(c => c.id)).toEqual(["a", "c"]);
    expect(colNames(target)).toEqual(["To do", "Done"]);
  });

  it("writes a renamed column back to content", async () => {
    const { target, content } = setup([col("a", "To do")]);
    const input = target.querySelector<HTMLInputElement>(".col-name")!;
    input.value = "Backlog";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    expect(content.columns[0].name).toBe("Backlog");
  });
});

describe("cards", () => {
  it("adds a blank card to the right column", async () => {
    const { target, content } = setup([col("a", "To do"), col("b", "Doing")]);
    target.querySelectorAll<HTMLButtonElement>(".add-card")[1].click();
    await flush();
    expect(content.columns[0].cards).toHaveLength(0);
    expect(content.columns[1].cards).toHaveLength(1);
  });

  it("removes the card whose × was pressed", async () => {
    const { target, content } = setup([
      col("a", "To do", [{ id: "c1", title: "one" }, { id: "c2", title: "two" }]),
    ]);
    target.querySelectorAll<HTMLButtonElement>(".card-del")[0].click();
    await flush();
    expect(content.columns[0].cards.map(c => c.id)).toEqual(["c2"]);
  });

  it("writes an edited card title back to content", async () => {
    const { target, content } = setup([col("a", "To do", [{ id: "c1", title: "" }])]);
    const ta = target.querySelector<HTMLTextAreaElement>(".card-text")!;
    ta.value = "write tests";
    ta.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    expect(content.columns[0].cards[0].title).toBe("write tests");
  });
});

describe("dragging a card", () => {
  it("moves it into the column it was dropped on", async () => {
    const { target, content } = setup([
      col("a", "To do", [{ id: "c1", title: "task" }]),
      col("b", "Doing"),
    ]);
    const handle = target.querySelector('.card[data-card-id="c1"] .handle')!;
    const destination = target.querySelector('.column[data-col-id="b"]')!;

    await drag(handle, destination);

    expect(cardTitles(target, "a")).toEqual([]);
    expect(content.columns[1].cards.map(c => c.id)).toEqual(["c1"]);
  });

  it("drops it before the card it was released over", async () => {
    const { target, content } = setup([
      col("a", "To do", [{ id: "c1", title: "one" }, { id: "c2", title: "two" }, { id: "c3", title: "three" }]),
    ]);
    const handle = target.querySelector('.card[data-card-id="c3"] .handle')!;
    const overFirst = target.querySelector('.card[data-card-id="c1"]')!;

    await drag(handle, overFirst);

    expect(content.columns[0].cards.map(c => c.id)).toEqual(["c3", "c1", "c2"]);
  });

  // Releasing over the card's own body must not be read as "insert before self".
  it("leaves the order alone when dropped on itself", async () => {
    const { target, content } = setup([
      col("a", "To do", [{ id: "c1", title: "one" }, { id: "c2", title: "two" }]),
    ]);
    const handle = target.querySelector('.card[data-card-id="c1"] .handle')!;
    const itself = target.querySelector('.card[data-card-id="c1"]')!;

    await drag(handle, itself);

    expect(content.columns[0].cards.map(c => c.id)).toEqual(["c1", "c2"]);
  });

  it("does nothing when released outside any column", async () => {
    const { target, content } = setup([col("a", "To do", [{ id: "c1", title: "task" }])]);
    const handle = target.querySelector('.card[data-card-id="c1"] .handle')!;

    await drag(handle, null);

    expect(content.columns[0].cards.map(c => c.id)).toEqual(["c1"]);
  });
});

describe("dragging a column", () => {
  it("reorders onto the column it was dropped on", async () => {
    const { target, content } = setup([col("a", "A"), col("b", "B"), col("c", "C")]);
    const handle = target.querySelector('.column[data-col-id="c"] .col-header .handle')!;
    const destination = target.querySelector('.column[data-col-id="a"]')!;

    await drag(handle, destination);

    expect(content.columns.map(c => c.id)).toEqual(["c", "a", "b"]);
  });

  it("does nothing when dropped on itself", async () => {
    const { target, content } = setup([col("a", "A"), col("b", "B")]);
    const handle = target.querySelector('.column[data-col-id="a"] .col-header .handle')!;
    const itself = target.querySelector('.column[data-col-id="a"]')!;

    await drag(handle, itself);

    expect(content.columns.map(c => c.id)).toEqual(["a", "b"]);
  });
});

describe("drag ghost", () => {
  it("is absent until a drag starts", () => {
    const { target } = setup([col("a", "A", [{ id: "c1", title: "task" }])]);
    expect(document.querySelector(".ghost")).toBeNull();
    expect(target).toBeTruthy();
  });

  it("shows the card's text and follows the pointer, then disappears", async () => {
    const { target } = setup([col("a", "A", [{ id: "c1", title: "task" }])]);
    const handle = target.querySelector('.card[data-card-id="c1"] .handle')!;

    handle.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 5, clientY: 5 }));
    await flush();
    const ghost = document.querySelector<HTMLElement>(".ghost")!;
    expect(ghost.textContent!.trim()).toBe("task");

    window.dispatchEvent(new PointerEvent("pointermove", { clientX: 50, clientY: 60 }));
    await flush();
    // rendered offset from the cursor so it cannot block elementFromPoint
    expect(document.querySelector<HTMLElement>(".ghost")!.style.left).toBe("62px");
    expect(document.querySelector<HTMLElement>(".ghost")!.style.top).toBe("72px");

    vi.spyOn(document, "elementFromPoint").mockReturnValue(null as unknown as Element);
    window.dispatchEvent(new PointerEvent("pointerup", { clientX: 50, clientY: 60 }));
    await flush();
    expect(document.querySelector(".ghost")).toBeNull();
  });

  it("labels a blank card rather than showing an empty ghost", async () => {
    const { target } = setup([col("a", "A", [{ id: "c1", title: "" }])]);
    const handle = target.querySelector('.card[data-card-id="c1"] .handle')!;
    handle.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 5, clientY: 5 }));
    await flush();
    expect(document.querySelector(".ghost")!.textContent!.trim()).toBe("Card");
  });

  it("uses the column name when dragging a column", async () => {
    const { target } = setup([col("a", "Backlog")]);
    const handle = target.querySelector('.column[data-col-id="a"] .col-header .handle')!;
    handle.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 5, clientY: 5 }));
    await flush();
    expect(document.querySelector(".ghost")!.textContent!.trim()).toBe("Backlog");
  });
});

describe("moving with the keyboard", () => {
  const key = (el: Element, k: string) =>
    el.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true, cancelable: true }));

  it("moves a column left and right from its handle", async () => {
    const { target, content } = setup([col("a", "To do"), col("b", "Doing"), col("c", "Done")]);
    key(target.querySelectorAll(".col-header .handle")[0], "ArrowRight");
    await flush();
    expect(content.columns.map(c => c.id)).toEqual(["b", "a", "c"]);
    expect(colNames(target)).toEqual(["Doing", "To do", "Done"]);
    key(target.querySelector('.column[data-col-id="a"] .col-header .handle')!, "ArrowLeft");
    await flush();
    expect(colNames(target)).toEqual(["To do", "Doing", "Done"]);
  });

  it("moves a card up, down and across columns", async () => {
    const { target, content } = setup([
      col("a", "To do", [{ id: "x", title: "X" }, { id: "y", title: "Y" }]),
      col("b", "Doing"),
    ]);
    key(target.querySelector('[data-card-id="y"] .handle')!, "ArrowUp");
    await flush();
    expect(content.columns[0].cards.map(c => c.id)).toEqual(["y", "x"]);
    expect(cardTitles(target, "a")).toEqual(["Y", "X"]);
    key(target.querySelector('[data-card-id="y"] .handle')!, "ArrowRight");
    await flush();
    expect([cardTitles(target, "a"), cardTitles(target, "b")]).toEqual([["X"], ["Y"]]);
  });

  it("does nothing at either end", async () => {
    const { target, content } = setup([col("a", "To do", [{ id: "x", title: "X" }])]);
    key(target.querySelector(".col-header .handle")!, "ArrowLeft");
    key(target.querySelector('[data-card-id="x"] .handle')!, "ArrowDown");
    key(target.querySelector('[data-card-id="x"] .handle')!, "ArrowRight");
    await flush();
    expect(content.columns.map(c => c.cards.map(k => k.id))).toEqual([["x"]]);
  });
});
