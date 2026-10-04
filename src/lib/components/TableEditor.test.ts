// @vitest-environment happy-dom
//
// Masked columns hold credentials, so the assertions here are deliberately about
// what must NOT be in the DOM: a masked cell renders dots, and the plaintext is
// absent from the rendered markup until the user explicitly reveals it.
import { describe, it, expect, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import TableEditor from "./TableEditor.svelte";
import type { TableContent } from "$lib/tableParsers";

/// happy-dom exposes navigator.clipboard as a read-only getter, so it has to be
/// redefined rather than assigned.
function stubClipboard(impl: Record<string, unknown>) {
  Object.defineProperty(navigator, "clipboard", {
    value: impl,
    configurable: true,
    writable: true,
  });
}

const SECRET = "hunter2-super-secret";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

function contentWithSecret(): TableContent {
  return {
    columns: [
      { id: "site", name: "Site" },
      { id: "pw", name: "Password", type: "masked" },
    ],
    rows: [{ id: "r1", cells: { site: "github.com", pw: SECRET } }],
  };
}

async function setup(content: TableContent) {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(TableEditor, { target, props: { content } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  await new Promise(r => setTimeout(r, 0));
  return { target, content };
}

const buttonByLabel = (t: HTMLElement, label: string) =>
  t.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`);

describe("TableEditor masked columns", () => {
  it("does not render a masked value in the DOM", async () => {
    const { target } = await setup(contentWithSecret());
    expect(target.textContent).not.toContain(SECRET);
    expect(target.innerHTML).not.toContain(SECRET);
  });

  it("shows a fixed-width mask that does not leak the length", async () => {
    const { target } = await setup(contentWithSecret());
    const shown = target.querySelector(".secret-value")?.textContent?.trim() ?? "";
    expect(shown).toBe("••••••••");
    expect(shown.length).not.toBe(SECRET.length);
  });

  it("reveals the value only after the user asks", async () => {
    const { target } = await setup(contentWithSecret());
    buttonByLabel(target, "Show value")!.click();
    await new Promise(r => setTimeout(r, 0));
    expect(target.textContent).toContain(SECRET);
  });

  it("hides the value again on a second toggle", async () => {
    const { target } = await setup(contentWithSecret());
    buttonByLabel(target, "Show value")!.click();
    await new Promise(r => setTimeout(r, 0));
    buttonByLabel(target, "Hide value")!.click();
    await new Promise(r => setTimeout(r, 0));
    expect(target.textContent).not.toContain(SECRET);
  });

  it("renders unmasked columns normally", async () => {
    const { target } = await setup(contentWithSecret());
    expect(target.textContent).toContain("github.com");
  });

  it("copies the real value, not the mask", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    stubClipboard({ writeText, readText: vi.fn() });
    const { target } = await setup(contentWithSecret());
    buttonByLabel(target, "Copy value")!.click();
    await new Promise(r => setTimeout(r, 0));
    expect(writeText).toHaveBeenCalledWith(SECRET);
  });

  it("clears the clipboard afterwards, but only if the secret is still on it", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    const readText = vi.fn().mockResolvedValue(SECRET);
    stubClipboard({ writeText, readText });

    const target = document.createElement("div");
    document.body.appendChild(target);
    const app = mount(TableEditor, { target, props: { content: contentWithSecret() } });
    cleanup = () => { try { unmount(app); } catch { /* noise */ } };
    await vi.advanceTimersByTimeAsync(0);

    buttonByLabel(target, "Copy value")!.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(writeText).toHaveBeenCalledWith(SECRET);

    await vi.advanceTimersByTimeAsync(30_000);
    expect(writeText).toHaveBeenLastCalledWith("");
    vi.useRealTimers();
  });

  it("still clears the clipboard after the user navigates away", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    const readText = vi.fn().mockResolvedValue(SECRET);
    stubClipboard({ writeText, readText });

    const target = document.createElement("div");
    document.body.appendChild(target);
    const app = mount(TableEditor, { target, props: { content: contentWithSecret() } });
    cleanup = () => { try { unmount(app); } catch { /* noise */ } };
    await vi.advanceTimersByTimeAsync(0);

    buttonByLabel(target, "Copy value")!.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(writeText).toHaveBeenCalledWith(SECRET);

    unmount(app); // navigating away must not strand the secret on the clipboard
    cleanup = null;
    await vi.advanceTimersByTimeAsync(30_000);

    expect(writeText).toHaveBeenLastCalledWith("");
    vi.useRealTimers();
  });

  it("leaves the clipboard alone if the user copied something else meanwhile", async () => {
    vi.useFakeTimers();
    const writeText = vi.fn().mockResolvedValue(undefined);
    const readText = vi.fn().mockResolvedValue("something the user copied later");
    stubClipboard({ writeText, readText });

    const target = document.createElement("div");
    document.body.appendChild(target);
    const app = mount(TableEditor, { target, props: { content: contentWithSecret() } });
    cleanup = () => { try { unmount(app); } catch { /* noise */ } };
    await vi.advanceTimersByTimeAsync(0);

    buttonByLabel(target, "Copy value")!.click();
    await vi.advanceTimersByTimeAsync(30_000);

    expect(writeText).toHaveBeenCalledTimes(1); // the copy only; no clobbering blank
    vi.useRealTimers();
  });
});
