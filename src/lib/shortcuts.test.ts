// @vitest-environment happy-dom
import { describe, it, expect } from "vitest";
import { shortcutFor, findBelongsToNote } from "./shortcuts";

function press(key: string, init: KeyboardEventInit = {}, target: HTMLElement = document.body) {
  const e = new KeyboardEvent("keydown", { key, bubbles: true, ...init });
  Object.defineProperty(e, "target", { value: target });
  return shortcutFor(e);
}

describe("shortcutFor", () => {
  it("maps the list shortcuts", () => {
    expect(press("n", { ctrlKey: true })).toBe("new-note");
    expect(press("n", { metaKey: true })).toBe("new-note");
    expect(press("N", { ctrlKey: true, shiftKey: true })).toBe("new-folder");
    expect(press("f", { ctrlKey: true })).toBe("find");
    expect(press("Delete")).toBe("delete");
    expect(press("Escape")).toBe("escape");
    expect(press("c", { ctrlKey: true })).toBe("copy");
    expect(press("x", { metaKey: true })).toBe("cut");
    expect(press("V", { ctrlKey: true })).toBe("paste");
  });

  it("leaves Ctrl+C to the browser while text is selected", () => {
    const p = document.createElement("p");
    p.textContent = "pick me";
    document.body.appendChild(p);
    window.getSelection()!.selectAllChildren(p);
    expect(press("c", { ctrlKey: true })).toBeNull();
    expect(press("x", { ctrlKey: true })).toBeNull();
    window.getSelection()!.removeAllRanges();
    p.remove();
    expect(press("c", { ctrlKey: true })).toBe("copy");
  });

  it("leaves plain typing and unrelated combos alone", () => {
    expect(press("n")).toBeNull();
    expect(press("s", { ctrlKey: true })).toBeNull();
    expect(press("Delete", { ctrlKey: true })).toBeNull();
  });

  it("ignores keys typed into a field, except Escape", () => {
    const input = document.createElement("input");
    const area = document.createElement("textarea");
    const editable = document.createElement("div");
    editable.contentEditable = "true";
    // happy-dom does not derive isContentEditable from the attribute.
    Object.defineProperty(editable, "isContentEditable", { value: true });
    for (const field of [input, area, editable]) {
      expect(press("Delete", {}, field)).toBeNull();
      expect(press("n", { ctrlKey: true }, field)).toBeNull();
      expect(press("f", { ctrlKey: true }, field)).toBeNull();
      for (const k of ["c", "x", "v"]) expect(press(k, { ctrlKey: true }, field)).toBeNull();
      expect(press("Escape", {}, field)).toBe("escape");
    }
  });
});

describe("findBelongsToNote", () => {
  it("is the list's when no note is open", () => {
    expect(findBelongsToNote(document.body)).toBe(false);
  });

  it("is the note's when one is open, unless focus is in the list", () => {
    const note = document.createElement("div");
    note.dataset.findRoot = "";
    const list = document.createElement("div");
    list.dataset.listPane = "";
    const search = document.createElement("input");
    list.appendChild(search);
    document.body.append(note, list);
    expect(findBelongsToNote(document.body)).toBe(true);
    expect(findBelongsToNote(note)).toBe(true);
    expect(findBelongsToNote(search)).toBe(false);
    note.remove();
    list.remove();
  });
});
