// @vitest-environment happy-dom
//
// The note route is the app's largest component and holds every path that can
// lose a user's writing: the draft autosave, the save itself, the dirty
// navigation guard, delete, and the round-trip safety hold. Those are what this
// covers — not the chrome around them.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import { writable } from "svelte/store";

// vi.mock is hoisted above every top-level binding, so anything the factories
// close over has to be hoisted with it.
const h = vi.hoisted(() => ({
  navGuard: null as null | ((n: { cancel: () => void; to: { url: URL } | null }) => void),
  pageState: { params: { id: "n1" }, url: new URL("http://localhost/note/n1") },
}));
const pageState = h.pageState;

vi.mock("$app/navigation", () => ({
  goto: vi.fn(async () => {}),
  beforeNavigate: vi.fn((fn: never) => { h.navGuard = fn; }),
  afterNavigate: vi.fn(),
}));

vi.mock("$app/state", () => ({ page: h.pageState }));

vi.mock("$lib/stores/layout", () => ({ isDesktop: writable(true) }));

vi.mock("$lib/stores/notes", () => ({
  refreshNotes: vi.fn(async () => {}),
  notes: writable([]),
  bgImages: writable({}),
  totalNotes: writable(0),
}));

vi.mock("$lib/tauri", () => ({
  LOCKED: "NOTE_LOCKED",
  WRONG_PASSWORD: "WRONG_PASSWORD",
  noteGet: vi.fn(),
  noteCreate: vi.fn(async () => ({ id: "created-1" })),
  noteUpdate: vi.fn(async () => {}),
  notesDelete: vi.fn(async () => {}),
  noteUnlock: vi.fn(async () => {}),
  noteLock: vi.fn(async () => {}),
  noteProtect: vi.fn(async () => {}),
  noteUnprotect: vi.fn(async () => {}),
  noteChangePassword: vi.fn(async () => {}),
  noteRecover: vi.fn(async () => {}),
  noteAddRecovery: vi.fn(async () => "AAAA-BBBB"),
  noteDraftSave: vi.fn(async () => {}),
  noteDraftGet: vi.fn(async () => null),
  noteDraftDiscard: vi.fn(async () => {}),
}));

import { goto } from "$app/navigation";
import {
  noteGet, noteCreate, noteUpdate, notesDelete,
  noteDraftSave, noteDraftGet, noteDraftDiscard, LOCKED,
} from "$lib/tauri";
import { refreshNotes } from "$lib/stores/notes";
import NotePage from "./+page.svelte";

const DRAFT_DEBOUNCE_MS = 800;

const detail = (over: Record<string, unknown> = {}) => ({
  id: "n1",
  kind: "checklist",
  title: "Groceries",
  content: { items: [] },
  tags: ["shopping"],
  show_preview: true,
  bg_color: null,
  bg_image: null,
  has_note_password: false,
  updated_at: 1_700_000_000,
  ...over,
});

let cleanup: (() => void) | null = null;
const flush = async () => { await Promise.resolve(); await new Promise(r => setTimeout(r, 0)); };
/// Several awaits deep: persist() -> refreshNotes() -> rebaseline().
const settle = async () => { for (let i = 0; i < 6; i++) await flush(); };
const wait = (ms: number) => new Promise(r => setTimeout(r, ms));

async function setup(id = "n1", search = "") {
  pageState.params = { id };
  pageState.url = new URL(`http://localhost/note/${id}${search}`);
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(NotePage, { target, props: {} });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  await flush();
  await flush();
  return target;
}

const titleBox = (t: HTMLElement) =>
  t.querySelector<HTMLTextAreaElement>("textarea.title-input")!;

async function typeTitle(t: HTMLElement, value: string) {
  const el = titleBox(t);
  el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  await flush();
}

beforeEach(() => {
  vi.clearAllMocks();
  h.navGuard = null;
  // clearAllMocks resets recorded calls but keeps implementations, so a
  // mockRejectedValue set by one test would leak into every later one.
  vi.mocked(noteGet).mockResolvedValue(detail() as never);
  vi.mocked(noteDraftGet).mockResolvedValue(null as never);
  vi.mocked(noteUpdate).mockResolvedValue(undefined as never);
  vi.mocked(noteCreate).mockResolvedValue({ id: "created-1" } as never);
  vi.mocked(notesDelete).mockResolvedValue(undefined as never);
  vi.mocked(noteDraftSave).mockResolvedValue(undefined as never);
  vi.mocked(noteDraftDiscard).mockResolvedValue(undefined as never);
});

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

describe("opening a note", () => {
  it("loads the note by the id in the route", async () => {
    await setup("n1");
    expect(noteGet).toHaveBeenCalledWith("n1");
    expect(titleBox(document.body).value).toBe("Groceries");
  });

  it("does not call the backend for a brand-new note", async () => {
    await setup("new", "?kind=checklist");
    expect(noteGet).not.toHaveBeenCalled();
    expect(titleBox(document.body).value).toBe("");
  });

  it("surfaces a load failure instead of showing an empty note", async () => {
    vi.mocked(noteGet).mockRejectedValue(new Error("db is gone"));
    const t = await setup("n1");
    expect(t.textContent).toContain("db is gone");
  });

  it("shows the lock gate rather than an error for a protected note", async () => {
    vi.mocked(noteGet).mockRejectedValue(LOCKED);
    const t = await setup("n1");
    expect(t.textContent).not.toContain("Error");
    expect(t.querySelector('input[type="password"]')).toBeTruthy();
  });
});

describe("draft autosave", () => {
  it("writes a draft after the user stops typing", async () => {
    const t = await setup("n1");
    await typeTitle(t, "Groceries and more");

    expect(noteDraftSave).not.toHaveBeenCalled();
    await wait(DRAFT_DEBOUNCE_MS + 150);

    expect(noteDraftSave).toHaveBeenCalledWith("n1", expect.objectContaining({
      title: "Groceries and more",
    }));
  });

  it("debounces rather than writing on every keystroke", async () => {
    const t = await setup("n1");
    await typeTitle(t, "a");
    await wait(200);
    await typeTitle(t, "ab");
    await wait(200);
    await typeTitle(t, "abc");
    await wait(DRAFT_DEBOUNCE_MS + 150);

    expect(noteDraftSave).toHaveBeenCalledTimes(1);
    expect(noteDraftSave).toHaveBeenCalledWith("n1", expect.objectContaining({ title: "abc" }));
  });

  // A new note has no id to key a draft on; a protected one would put plaintext
  // outside the password layer.
  it("never drafts a brand-new note", async () => {
    const t = await setup("new", "?kind=checklist");
    await typeTitle(t, "Untitled thoughts");
    await wait(DRAFT_DEBOUNCE_MS + 150);
    expect(noteDraftSave).not.toHaveBeenCalled();
  });

  it("never drafts a password-protected note", async () => {
    vi.mocked(noteGet).mockResolvedValue(detail({ has_note_password: true }) as never);
    const t = await setup("n1");
    await typeTitle(t, "secret plans");
    await wait(DRAFT_DEBOUNCE_MS + 150);
    expect(noteDraftSave).not.toHaveBeenCalled();
  });

  it("stops the pending write when the component goes away", async () => {
    const t = await setup("n1");
    await typeTitle(t, "half a thought");
    cleanup?.();
    cleanup = null;
    await wait(DRAFT_DEBOUNCE_MS + 150);
    expect(noteDraftSave).not.toHaveBeenCalled();
  });
});

describe("a draft found on open", () => {
  const draft = {
    title: "Groceries (edited)", content: { items: [] }, tags: ["shopping"],
    updated_at: Math.floor(Date.now() / 1000) - 3600,
  };

  it("is offered, not applied silently", async () => {
    vi.mocked(noteDraftGet).mockResolvedValue(draft as never);
    const t = await setup("n1");
    // the saved note still shows until the user chooses
    expect(titleBox(t).value).toBe("Groceries");
    expect(t.textContent!.toLowerCase()).toMatch(/unsaved changes from/);
  });

  it("is not offered when it matches the saved note", async () => {
    vi.mocked(noteDraftGet).mockResolvedValue({
      title: "Groceries", content: { items: [] }, tags: ["shopping"],
      updated_at: Math.floor(Date.now() / 1000) - 3600,
    } as never);
    const t = await setup("n1");
    expect(t.textContent!.toLowerCase()).not.toMatch(/unsaved changes from/);
  });

  it("does not block opening the note when the draft cannot be read", async () => {
    vi.mocked(noteDraftGet).mockRejectedValue(new Error("corrupt draft"));
    const t = await setup("n1");
    expect(titleBox(t).value).toBe("Groceries");
    expect(t.textContent).not.toContain("corrupt draft");
  });
});

describe("saving", () => {
  it("updates an existing note", async () => {
    const t = await setup("n1");
    await typeTitle(t, "Groceries v2");

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true }));
    await settle();

    expect(noteUpdate).toHaveBeenCalledWith("n1", expect.objectContaining({ title: "Groceries v2" }));
    expect(noteCreate).not.toHaveBeenCalled();
    expect(refreshNotes).toHaveBeenCalledWith({ withBackgrounds: true });
  });

  it("creates a brand-new note and rebinds the route to the real id", async () => {
    await setup("new", "?kind=checklist");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true }));
    await settle();

    expect(noteCreate).toHaveBeenCalled();
    // Without the rebind the next save would create a duplicate.
    expect(goto).toHaveBeenCalledWith("/note/created-1", expect.objectContaining({ replaceState: true }));
  });

  it("accepts Cmd+S as well as Ctrl+S", async () => {
    const t = await setup("n1");
    await typeTitle(t, "mac save");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", metaKey: true }));
    await settle();
    expect(noteUpdate).toHaveBeenCalled();
  });

  it("shows the error and stays put when the write fails", async () => {
    vi.mocked(noteUpdate).mockRejectedValue(new Error("disk full"));
    const t = await setup("n1");
    await typeTitle(t, "will not save");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true }));
    await settle();

    expect(t.textContent).toContain("disk full");
    expect(goto).not.toHaveBeenCalled();
  });

  it("asks for the password instead of reporting an error when the note locked mid-edit", async () => {
    vi.mocked(noteUpdate).mockRejectedValue(LOCKED);
    const t = await setup("n1");
    await typeTitle(t, "locked out");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true }));
    await settle();

    expect(t.querySelector('input[type="password"]')).toBeTruthy();
  });

  it("ignores every other Ctrl shortcut", async () => {
    const t = await setup("n1");
    await typeTitle(t, "x");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "p", ctrlKey: true }));
    await flush();
    expect(noteUpdate).not.toHaveBeenCalled();
  });

  it("stops listening for Ctrl+S once the component is gone", async () => {
    const t = await setup("n1");
    await typeTitle(t, "x");
    cleanup?.();
    cleanup = null;
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true }));
    await settle();
    expect(noteUpdate).not.toHaveBeenCalled();
  });
});

describe("the unsaved-changes guard", () => {
  it("lets a clean note navigate away untouched", async () => {
    await setup("n1");
    const cancel = vi.fn();
    h.navGuard?.({ cancel, to: { url: new URL("http://localhost/") } });
    expect(cancel).not.toHaveBeenCalled();
  });

  it("stops a dirty note from leaving", async () => {
    const t = await setup("n1");
    await typeTitle(t, "unsaved work");
    const cancel = vi.fn();
    h.navGuard?.({ cancel, to: { url: new URL("http://localhost/") } });
    expect(cancel).toHaveBeenCalled();
  });

  it("lets go once the note has been saved", async () => {
    const t = await setup("n1");
    await typeTitle(t, "saved work");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "s", ctrlKey: true }));
    await settle();

    const cancel = vi.fn();
    h.navGuard?.({ cancel, to: { url: new URL("http://localhost/") } });
    expect(cancel).not.toHaveBeenCalled();
  });
});

describe("deleting", () => {
  // The overflow menu's Delete used to only close the menu.
  it("actually deletes, refreshes the list and leaves the note", async () => {
    const t = await setup("n1");

    t.querySelector<HTMLButtonElement>('[aria-label="More options"]')!.click();
    await flush();

    const del = [...t.querySelectorAll<HTMLButtonElement>("button")]
      .find(b => /delete/i.test(b.textContent ?? ""));
    expect(del, "no Delete control found in the note menu").toBeTruthy();
    del!.click();
    await flush();

    document.querySelector<HTMLButtonElement>(".btn-confirm")!.click();
    await flush();
    await flush();

    expect(notesDelete).toHaveBeenCalledWith(["n1"]);
    expect(refreshNotes).toHaveBeenCalled();
    expect(goto).toHaveBeenCalledWith("/");
  });
});

describe("round-trip safety hold", () => {
  const frontMatter = "---\ntitle: x\n---\n\nbody text";

  it("holds a note whose markdown the editor cannot represent", async () => {
    vi.mocked(noteGet).mockResolvedValue(
      detail({ kind: "document", content: { body: frontMatter } }) as never,
    );
    const t = await setup("n1");
    expect(t.textContent!.toLowerCase()).toMatch(/formatting the editor can't keep/);
  });

  it("leaves an ordinary markdown note alone", async () => {
    vi.mocked(noteGet).mockResolvedValue(
      detail({ kind: "document", content: { body: "# Title\n\nplain paragraph" } }) as never,
    );
    const t = await setup("n1");
    expect(t.textContent!.toLowerCase()).not.toMatch(/formatting the editor can't keep/);
  });

  it("does not scan structured kinds for markdown constructs", async () => {
    vi.mocked(noteGet).mockResolvedValue(
      detail({ kind: "checklist", content: { items: [{ id: "a", text: "---", checked: false, children: [] }] } }) as never,
    );
    const t = await setup("n1");
    expect(t.textContent!.toLowerCase()).not.toMatch(/formatting the editor can't keep/);
  });
});

describe("discarding a draft", () => {
  it("clears it on the backend too, so it is not re-offered", async () => {
    vi.mocked(noteDraftGet).mockResolvedValue({
      title: "Groceries (edited)", content: { items: [] }, tags: [],
      updated_at: Math.floor(Date.now() / 1000) - 3600,
    } as never);
    const t = await setup("n1");

    const discard = [...t.querySelectorAll<HTMLButtonElement>("button")]
      .find(b => /keep saved/i.test(b.textContent ?? ""));
    expect(discard, "no keep-saved control offered for the pending draft").toBeTruthy();
    discard!.click();
    await flush();

    expect(noteDraftDiscard).toHaveBeenCalledWith("n1");
  });
});
