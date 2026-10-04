// @vitest-environment happy-dom
//
// Nothing moved focus into this dialog on open, and nothing restored it on
// close — a keyboard/screen-reader user had no indication the dialog opened.
import { describe, it, expect, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import NewNoteModal from "./NewNoteModal.svelte";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

function setup() {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const onclose = vi.fn();
  const app = mount(NewNoteModal, { target, props: { onclose } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target, onclose };
}

describe("NewNoteModal", () => {
  it("focuses the first kind option on mount", async () => {
    const { target } = setup();
    await new Promise(r => setTimeout(r, 0));
    const first = target.querySelector(".kind-row");
    expect(document.activeElement).toBe(first);
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
