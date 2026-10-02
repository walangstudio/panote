// @vitest-environment happy-dom
import { describe, it, expect } from "vitest";
import { shortcutFor } from "./shortcuts";

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
      expect(press("Escape", {}, field)).toBe("escape");
    }
  });
});
