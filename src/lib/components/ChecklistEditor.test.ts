// @vitest-environment happy-dom
//
// The checklist edits a nested tree in place and reassigns `content` to trigger
// Svelte's reactivity. Every mutation has to reach the bound object, or the note
// saves without the user's edit.
import { describe, it, expect, afterEach } from "vitest";
import { mount, unmount } from "svelte";
import ChecklistEditor from "./ChecklistEditor.svelte";

interface CheckItem { id: string; text: string; checked: boolean; children: CheckItem[] }

const item = (over: Partial<CheckItem> = {}): CheckItem => ({
  id: over.id ?? crypto.randomUUID(),
  text: "",
  checked: false,
  children: [],
  ...over,
});

let cleanup: (() => void) | null = null;
const flush = () => new Promise(r => setTimeout(r, 0));

function setup(items: CheckItem[] = []) {
  const content = { items };
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(ChecklistEditor, { target, props: { content } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target, content };
}

const rows = (t: HTMLElement) => [...t.querySelectorAll(".item")];
const texts = (t: HTMLElement) =>
  [...t.querySelectorAll<HTMLInputElement>(".text-input")].map(i => i.value);
const addBtn = (t: HTMLElement) => t.querySelector<HTMLButtonElement>(".add-btn")!;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

describe("adding and removing", () => {
  it("starts empty and still offers the add button", () => {
    const { target } = setup();
    expect(rows(target)).toHaveLength(0);
    expect(addBtn(target)).toBeTruthy();
  });

  it("adds a blank item", async () => {
    const { target } = setup();
    addBtn(target).click();
    await flush();
    expect(rows(target)).toHaveLength(1);
    expect(texts(target)).toEqual([""]);
  });

  it("appends rather than replacing", async () => {
    const { target } = setup([item({ text: "first" })]);
    addBtn(target).click();
    await flush();
    expect(texts(target)).toEqual(["first", ""]);
  });

  it("deletes the row the button belongs to, not the first one", async () => {
    const { target } = setup([item({ text: "a" }), item({ text: "b" }), item({ text: "c" })]);
    target.querySelectorAll<HTMLButtonElement>(".del")[1].click();
    await flush();
    expect(texts(target)).toEqual(["a", "c"]);
  });

  it("survives deleting every row", async () => {
    const { target } = setup([item({ text: "only" })]);
    target.querySelector<HTMLButtonElement>(".del")!.click();
    await flush();
    expect(rows(target)).toHaveLength(0);
  });
});

describe("checking off", () => {
  it("marks an item done and reflects it in the class", async () => {
    const { target, content } = setup([item({ text: "milk" })]);
    target.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
    await flush();
    expect(content.items[0].checked).toBe(true);
    expect(target.querySelector(".item")!.classList.contains("done")).toBe(true);
  });

  it("unchecks again", async () => {
    const { target, content } = setup([item({ text: "milk", checked: true })]);
    expect(target.querySelector(".item")!.classList.contains("done")).toBe(true);
    target.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click();
    await flush();
    expect(content.items[0].checked).toBe(false);
  });

  it("only touches the item that was clicked", async () => {
    const { target, content } = setup([item({ text: "a" }), item({ text: "b" })]);
    target.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1].click();
    await flush();
    expect(content.items.map(i => i.checked)).toEqual([false, true]);
  });
});

describe("sub-items", () => {
  it("nests a child under the row whose + was pressed", async () => {
    const { target, content } = setup([item({ text: "parent" })]);
    target.querySelector<HTMLButtonElement>(".add-sub")!.click();
    await flush();
    expect(content.items[0].children).toHaveLength(1);
    expect(texts(target)).toEqual(["parent", ""]);
  });

  it("indents each level further than the last", async () => {
    const child = item({ text: "child" });
    const { target } = setup([item({ text: "parent", children: [child] })]);
    const lists = [...target.querySelectorAll<HTMLElement>("ul")];
    expect(lists).toHaveLength(2);
    expect(lists[0].style.paddingLeft).toBe("0rem");
    expect(lists[1].style.paddingLeft).toBe("1.5rem");
  });

  it("adds a child to a row that has no children array yet", async () => {
    const orphan = { id: "x", text: "no children key", checked: false } as unknown as CheckItem;
    const { target, content } = setup([orphan]);
    target.querySelector<HTMLButtonElement>(".add-sub")!.click();
    await flush();
    expect(content.items[0].children).toHaveLength(1);
  });

  // A corrupt or hostile import could nest arbitrarily deep; rendering follows
  // the tree recursively, so the cap is what stops it blowing the stack.
  it("stops rendering past the depth cap", async () => {
    const MAX_DEPTH = 20;
    const deepest = item({ text: "leaf" });
    let node = deepest;
    for (let i = 0; i < MAX_DEPTH + 5; i++) node = item({ text: `d${i}`, children: [node] });

    const { target } = setup([node]);
    // depth 0..MAX_DEPTH inclusive get a <ul>; deeper levels are dropped.
    expect(target.querySelectorAll("ul")).toHaveLength(MAX_DEPTH + 1);
    expect(texts(target)).not.toContain("leaf");
  });

  it("renders a shallow tree in full", async () => {
    const tree = item({ text: "a", children: [item({ text: "b", children: [item({ text: "c" })] })] });
    const { target } = setup([tree]);
    expect(texts(target)).toEqual(["a", "b", "c"]);
  });
});

describe("editing text", () => {
  it("writes typed text back to the bound content", async () => {
    const { target, content } = setup([item({ text: "" })]);
    const input = target.querySelector<HTMLInputElement>(".text-input")!;
    input.value = "buy milk";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    expect(content.items[0].text).toBe("buy milk");
  });

  it("writes a nested item's text back too", async () => {
    const { target, content } = setup([item({ text: "parent", children: [item({ text: "" })] })]);
    const nested = target.querySelectorAll<HTMLInputElement>(".text-input")[1];
    nested.value = "sub";
    nested.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    expect(content.items[0].children[0].text).toBe("sub");
  });
});
