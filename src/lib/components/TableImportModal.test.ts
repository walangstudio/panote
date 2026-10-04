// @vitest-environment happy-dom
//
// Nothing moved focus into this dialog on open, nor restored it on close.
import { describe, it, expect, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import TableImportModal from "./TableImportModal.svelte";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

function setup() {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const onimport = vi.fn();
  const onclose = vi.fn();
  const app = mount(TableImportModal, { target, props: { columns: [], onimport, onclose } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target, onclose };
}

describe("TableImportModal", () => {
  it("focuses the close button on mount", async () => {
    const { target } = setup();
    await new Promise(r => setTimeout(r, 0));
    expect(document.activeElement).toBe(target.querySelector(".close"));
  });

  it("closes on Escape", async () => {
    const { onclose } = setup();
    await new Promise(r => setTimeout(r, 0));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("restores focus to whatever was focused before it opened", async () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();

    setup();
    await new Promise(r => setTimeout(r, 0));
    cleanup?.();
    cleanup = null;
    await new Promise(r => setTimeout(r, 0));
    expect(document.activeElement).toBe(opener);
  });
});
