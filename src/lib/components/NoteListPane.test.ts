// @vitest-environment happy-dom
//
// The list is the app's front door, so these drive it the way a user does:
// type in the search box, tick notes, press the toolbar buttons.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import type { NoteMetadata } from "$lib/tauri";

vi.mock("$lib/tauri", () => ({
  WRONG_PASSWORD: "WRONG_PASSWORD",
  noteList: vi.fn(async () => []),
  noteCount: vi.fn(async () => 0),
  notesDelete: vi.fn(async () => {}),
  notePin: vi.fn(async () => {}),
  noteProtect: vi.fn(async () => {}),
  noteUnprotect: vi.fn(async () => {}),
  noteChangePassword: vi.fn(async () => {}),
  notesProtect: vi.fn(async () => {}),
  notesUnprotect: vi.fn(async () => {}),
  folderCreate: vi.fn(), folderMove: vi.fn(), folderRename: vi.fn(), folderDelete: vi.fn(),
  noteSetFolder: vi.fn(), notesReorder: vi.fn(async () => {}), foldersReorder: vi.fn(async () => {}),
  trashList: vi.fn(async () => []),
  trashRestore: vi.fn(async () => {}),
  trashDelete: vi.fn(async () => {}),
  trashEmpty: vi.fn(async () => {}),
}));

// The shared stub exports plain functions, so navigation needs a spy to assert on.
vi.mock("$app/navigation", () => ({
  goto: vi.fn(async () => {}),
  beforeNavigate: vi.fn(),
  afterNavigate: vi.fn(),
}));

// The component reads `page.params.id`; the shared stub only carries `page.url`.
import { goto } from "$app/navigation";
import { page } from "$app/state";
(page as unknown as { params: Record<string, string> }).params = { id: "" };

import { noteList, notesUnprotect, notesReorder, foldersReorder, trashList, trashRestore, trashDelete, trashEmpty } from "$lib/tauri";
import type { TrashedNote } from "$lib/tauri";
import { notes, bgImages, sortPref } from "$lib/stores/notes";
import { resetListState, listFolder, listTrash } from "$lib/stores/listState";
import { folders } from "$lib/stores/folders";
import NoteListPane from "./NoteListPane.svelte";

const DAY = 86_400_000;

const note = (over: Partial<NoteMetadata> & { id: string }): NoteMetadata => ({
  kind: "document",
  title: "Untitled",
  tags: [],
  created_at: 0,
  updated_at: Math.floor(Date.now() / 1000),
  has_note_password: false,
  pinned: false,
  show_preview: true,
  ...over,
});

const fixtures: NoteMetadata[] = [
  note({ id: "n1", title: "Groceries", tags: ["shopping"], preview_text: "milk eggs and bread" }),
  note({ id: "n2", title: "Secret plans", tags: ["private"], has_note_password: true }),
  note({ id: "n3", title: "Recipes", tags: ["food"], preview_text: "sourdough starter notes" }),
  note({ id: "n4", title: "Vault", has_note_password: true }),
];

let cleanup: (() => void) | null = null;
const flush = () => new Promise(r => setTimeout(r, 0));

async function setup(list: NoteMetadata[] = fixtures) {
  vi.mocked(noteList).mockResolvedValue(list);
  notes.set(list);
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(NoteListPane, { target, props: {} });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  await flush();
  return target;
}

async function type(target: HTMLElement, query: string) {
  const input = target.querySelector<HTMLInputElement>("input.search")!;
  input.value = query;
  input.dispatchEvent(new Event("input", { bubbles: true }));
  await flush();
}

const titles = (t: HTMLElement) =>
  [...t.querySelectorAll(".note-info strong")].map(e => e.textContent);

beforeEach(() => {
  vi.clearAllMocks();
  // Search and selection now live in a module-level store so they survive the
  // component unmounting on mobile navigation — which means they also survive
  // between tests unless cleared.
  resetListState();
});
afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
  notes.set([]);
  bgImages.set({});
});

describe("search", () => {
  it("finds a note by its title", async () => {
    const t = await setup();
    await type(t, "recipes");
    expect(titles(t)).toEqual(["Recipes"]);
  });

  it("finds a note by its tag", async () => {
    const t = await setup();
    await type(t, "shopping");
    expect(titles(t)).toEqual(["Groceries"]);
  });

  // The preview line is printed on the card. A phrase the user can read off the
  // screen has to be findable, or the search box looks broken.
  it("finds a note by text shown in its preview", async () => {
    const t = await setup();
    await type(t, "sourdough");
    expect(titles(t)).toEqual(["Recipes"]);
  });

  it("never matches a password-protected note", async () => {
    const t = await setup();
    await type(t, "secret");
    expect(titles(t)).toEqual([]);
    expect(t.querySelector(".empty")).toBeTruthy();
  });

  // Excluding locked notes is deliberate, but silently returning nothing reads
  // as a bug. Say that something was held back.
  it("says how many locked notes a search is hiding", async () => {
    const t = await setup();
    await type(t, "secret");
    expect(t.textContent).toContain("2 locked notes hidden");
  });

  it("uses the singular for a single hidden note", async () => {
    const t = await setup([fixtures[0], fixtures[1]]);
    await type(t, "zzz");
    expect(t.textContent).toContain("1 locked note hidden");
  });

  it("says nothing about locked notes when there is no query", async () => {
    const t = await setup();
    expect(t.textContent).not.toContain("locked note");
    expect(titles(t)).toHaveLength(4);
  });

  it("says nothing when the library has no locked notes", async () => {
    const t = await setup([fixtures[0], fixtures[2]]);
    await type(t, "recipes");
    expect(t.textContent).not.toContain("locked note");
  });
});

// Card height used to follow content, because the preview line and the tag row
// were only rendered when they had something in them - so a bare note sat
// noticeably shorter than one with both. Reserving the slots is what makes every
// card the same height.
//
// The height itself cannot be asserted here: vitest runs with `css: false`, so
// scoped styles are never applied and any getComputedStyle check would pass
// vacuously. What is asserted is the structural invariant the CSS relies on.
describe("uniform card shape", () => {
  const slots = (t: HTMLElement) =>
    [...t.querySelectorAll(".note-card")].map(c => ({
      preview: !!c.querySelector(".preview-text"),
      tags: !!c.querySelector(".tags"),
    }));

  it("reserves both slots on a note with neither a preview nor tags", async () => {
    const t = await setup([note({ id: "bare", title: "Bare" })]);
    expect(slots(t)).toEqual([{ preview: true, tags: true }]);
  });

  it("reserves them identically whatever the note carries", async () => {
    const t = await setup([
      note({ id: "a", title: "Bare" }),
      note({ id: "b", title: "Preview only", preview_text: "some text" }),
      note({ id: "c", title: "Tags only", tags: ["x", "y"] }),
      note({ id: "d", title: "Both", preview_text: "some text", tags: ["x"] }),
      note({ id: "e", title: "Locked", has_note_password: true }),
    ]);
    expect(slots(t)).toEqual(Array(5).fill({ preview: true, tags: true }));
  });

  it("still shows the preview text when there is some", async () => {
    const t = await setup([note({ id: "a", preview_text: "milk and eggs" })]);
    expect(t.querySelector(".preview-text")!.textContent!.trim()).toBe("milk and eggs");
  });

  it("leaves the reserved preview empty rather than printing something", async () => {
    const t = await setup([note({ id: "a", title: "Bare" })]);
    expect(t.querySelector(".preview-text")!.textContent!.trim()).toBe("");
  });

  it("says a locked note is locked instead of leaking a preview", async () => {
    const t = await setup([
      note({ id: "a", has_note_password: true, preview_text: "SHOULD NOT SHOW" }),
    ]);
    const text = t.querySelector(".preview-text")!.textContent!;
    expect(text).toContain("Locked note");
    expect(text).not.toContain("SHOULD NOT SHOW");
  });

  it("honours show_preview being off", async () => {
    const t = await setup([
      note({ id: "a", show_preview: false, preview_text: "hidden please" }),
    ]);
    expect(t.querySelector(".preview-text")!.textContent!.trim()).toBe("");
  });

  // The tag row is clipped to one line in CSS; capping at 3 keeps the markup
  // honest about that rather than relying on overflow alone.
  it("never renders more than three tags", async () => {
    const t = await setup([note({ id: "a", tags: ["a", "b", "c", "d", "e"] })]);
    expect(t.querySelectorAll(".tag")).toHaveLength(3);
  });
});

// Explorer semantics: the list is a level, not a filter. You see the folders and
// notes directly inside where you are, and you go one level at a time.
//
// Before this, selecting a folder filtered notes but never showed subfolders, so
// a folder whose notes were one level down looked empty while its badge said 1 -
// and the list disagreed with folder_send, which sends the whole subtree.
describe("browsing folders like a file manager", () => {
  const folderRows = (t: HTMLElement) =>
    [...t.querySelectorAll(".folder-card .note-info strong")].map(e => e.textContent);
  const noteRows = (t: HTMLElement) =>
    [...t.querySelectorAll(".note-card:not(.folder-card) .note-info strong")].map(e => e.textContent);
  /// What a screen reader would announce: the icon span is aria-hidden, so its
  /// ligature text must not count.
  const crumbs = (t: HTMLElement) =>
    [...t.querySelectorAll(".crumb")].map(e =>
      [...e.childNodes]
        .filter(n => !(n instanceof HTMLElement && n.getAttribute("aria-hidden") === "true"))
        .map(n => n.textContent ?? "")
        .join("")
        .trim(),
    );

  const tree = [
    { id: "work", parent_id: null, name: "Work", note_count: 0 },
    { id: "clients", parent_id: "work", name: "Clients", note_count: 1 },
    { id: "personal", parent_id: null, name: "Personal", note_count: 0 },
  ];
  const library = [
    note({ id: "loose", title: "Unfiled" }),
    note({ id: "inwork", title: "Direct in Work", folder_id: "work" }),
    note({ id: "deep", title: "Deep in Clients", folder_id: "clients" }),
  ];

  beforeEach(() => { folders.set(tree); });
  afterEach(() => { folders.set([]); });

  it("shows top-level folders and unfiled notes at the root", async () => {
    const t = await setup(library);
    expect(folderRows(t)).toEqual(["Personal", "Work"]);
    expect(noteRows(t)).toEqual(["Unfiled"]);
  });

  it("does not show a foldered note at the root", async () => {
    const t = await setup(library);
    expect(noteRows(t)).not.toContain("Direct in Work");
    expect(noteRows(t)).not.toContain("Deep in Clients");
  });

  it("opens a folder to its subfolders and its own notes", async () => {
    const t = await setup(library);
    [...t.querySelectorAll<HTMLElement>(".folder-card")]
      .find(c => c.textContent!.includes("Work"))!.click();
    await flush();

    expect(folderRows(t)).toEqual(["Clients"]);
    expect(noteRows(t)).toEqual(["Direct in Work"]);
    // one level at a time: the nested note is not pulled up
    expect(noteRows(t)).not.toContain("Deep in Clients");
  });

  it("counts what opening the folder will actually show", async () => {
    const t = await setup(library);
    const work = [...t.querySelectorAll<HTMLElement>(".folder-card")]
      .find(c => c.textContent!.includes("Work"))!;
    // Work holds one subfolder and one note.
    expect(work.querySelector(".preview-text")!.textContent).toContain("2 items");
  });

  it("shows a breadcrumb only once you are inside something", async () => {
    const t = await setup(library);
    expect(crumbs(t)).toEqual([]);

    listFolder.set("clients");
    await flush();
    expect(crumbs(t)).toEqual(["Home", "Work", "Clients"]);
  });

  it("goes back up through the breadcrumb", async () => {
    const t = await setup(library);
    listFolder.set("clients");
    await flush();

    [...t.querySelectorAll<HTMLButtonElement>(".crumb")]
      .find(c => c.textContent!.trim() === "Work")!.click();
    await flush();
    expect(folderRows(t)).toEqual(["Clients"]);

    t.querySelector<HTMLButtonElement>(".crumb")!.click();
    await flush();
    expect(folderRows(t)).toEqual(["Personal", "Work"]);
  });

  it("says the folder is empty, not that the library is", async () => {
    const t = await setup(library);
    listFolder.set("personal");
    await flush();
    expect(t.querySelector(".empty")!.textContent).toContain("This folder is empty");
    expect(t.textContent).not.toContain("No notes yet");
  });

  // Explorer searches into subfolders; stopping at the current level would hide
  // the very thing being looked for.
  it("searches into subfolders", async () => {
    const t = await setup(library);
    listFolder.set("work");
    await flush();
    await type(t, "deep");
    expect(noteRows(t)).toEqual(["Deep in Clients"]);
  });

  it("does not let a search reach outside the folder you are in", async () => {
    const t = await setup(library);
    listFolder.set("work");
    await flush();
    await type(t, "unfiled");
    expect(noteRows(t)).toEqual([]);
  });

  it("hides folder rows while searching, since results span levels", async () => {
    const t = await setup(library);
    await type(t, "unfiled");
    expect(folderRows(t)).toEqual([]);
  });
});

// Hand-arranging the list. Only offered while the Manual sort is active: a drag
// under a date sort would appear to work and then be undone by the next refresh,
// which reads as the app losing the change.
// These dispatch pointer events straight at the handler, so they prove the
// reorder maths and the save, not that a real pointer arrives. That gap is why
// this shipped broken twice; e2e/custom-sort.spec.ts drives a real mouse and touch.
describe("arranging by hand", () => {
  const grips = (t: HTMLElement) => t.querySelectorAll<HTMLElement>(".drag-grip");
  const rowIds = (t: HTMLElement) =>
    [...t.querySelectorAll<HTMLElement>("[data-row-id]")].map(r => r.dataset.rowId);

  afterEach(() => { sortPref.set({ field: "updated", dir: "desc" }); });

  const three = [
    note({ id: "a", title: "Ay", sort_order: 0 }),
    note({ id: "b", title: "Bee", sort_order: 1 }),
    note({ id: "c", title: "Cee", sort_order: 2 }),
  ];

  /// One whole drag: pick a row up, move it over another, let go.
  async function drag(t: HTMLElement, gripIndex: number, ontoRowIndex: number) {
    const rows = [...t.querySelectorAll<HTMLElement>("[data-row-id]")];
    grips(t)[gripIndex].dispatchEvent(
      new PointerEvent("pointerdown", { bubbles: true, clientX: 0, clientY: 0 }),
    );
    await flush();
    vi.spyOn(document, "elementFromPoint").mockReturnValue(rows[ontoRowIndex]);
    window.dispatchEvent(new PointerEvent("pointermove", { clientX: 0, clientY: 200 }));
    await flush();
    window.dispatchEvent(new PointerEvent("pointerup", { clientX: 0, clientY: 200 }));
    await flush();
    await flush();
  }

  it("shows no grips under a date sort", async () => {
    sortPref.set({ field: "updated", dir: "desc" });
    const t = await setup(three);
    expect(grips(t)).toHaveLength(0);
  });

  // No mode to find first: choosing Custom is the whole gesture. Requiring a
  // second toggle meant the grips were never discovered and dragging looked broken.
  it("shows grips as soon as Custom is the sort", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup(three);
    expect(grips(t).length).toBe(3);
  });

  // Descending is the default direction, which is how every drop used to land
  // upside down.
  it("reorders on a drag and saves it without asking", async () => {
    sortPref.set({ field: "manual", dir: "desc" });
    const t = await setup(three);
    await drag(t, 0, 2);

    expect(notesReorder).toHaveBeenCalledTimes(1);
    expect(notesReorder).toHaveBeenCalledWith(["b", "c", "a"]);
  });

  it("tracks the new order mid-drag, before anything is saved", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup(three);
    const rows = [...t.querySelectorAll<HTMLElement>("[data-row-id]")];

    grips(t)[0].dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 0, clientY: 0 }));
    await flush();
    vi.spyOn(document, "elementFromPoint").mockReturnValue(rows[2]);
    window.dispatchEvent(new PointerEvent("pointermove", { clientX: 0, clientY: 200 }));
    await flush();
    await flush();

    expect(notesReorder).not.toHaveBeenCalled();
    expect(titles(t)).toEqual(["Bee", "Cee", "Ay"]);
    // Releasing here must commit exactly what the drag had arranged.
    window.dispatchEvent(new PointerEvent("pointerup", { clientX: 0, clientY: 200 }));
    await flush();
    await flush();
    expect(notesReorder).toHaveBeenCalledWith(["b", "c", "a"]);
  });

  // Rows stay clickable in Custom view, so only a real drag may swallow the
  // click - otherwise notes could not be opened at all while Custom is active.
  it("still opens a note on a plain click", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup(three);
    vi.mocked(goto).mockClear();
    t.querySelector<HTMLElement>(".note-card")!.click();
    await flush();
    expect(goto).toHaveBeenCalled();
  });

  it("does not open the note that was just dragged", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup(three);
    vi.mocked(goto).mockClear();
    const rows = [...t.querySelectorAll<HTMLElement>("[data-row-id]")];

    grips(t)[0].dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, clientX: 0, clientY: 0 }));
    await flush();
    vi.spyOn(document, "elementFromPoint").mockReturnValue(rows[2]);
    window.dispatchEvent(new PointerEvent("pointermove", { clientX: 0, clientY: 200 }));
    await flush();
    rows[0].click();
    await flush();

    expect(goto).not.toHaveBeenCalled();
  });

  it("moves a row with the arrow keys on its grip and announces it", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup(three);
    grips(t)[1].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
    await flush();
    await flush();
    expect(notesReorder).toHaveBeenCalledWith(["b", "a", "c"]);
    expect(t.parentElement!.querySelector("[aria-live]")!.textContent).toBe("Bee moved to position 1 of 3");
    expect(goto).not.toHaveBeenCalled();
  });

  it("does nothing past either end", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup(three);
    grips(t)[0].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowUp", bubbles: true }));
    grips(t)[2].dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
    await flush();
    expect(notesReorder).not.toHaveBeenCalled();
  });

  // Pinned notes stay above the rest; each section is arranged on its own.
  it("arranges pinned notes among themselves", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup([
      note({ id: "p", title: "Pin", pinned: true, sort_order: 0 }),
      note({ id: "q", title: "Queue", pinned: true, sort_order: 1 }),
      ...three,
    ]);
    await drag(t, 1, 0);
    expect(notesReorder).toHaveBeenCalledWith(["q", "p"]);
  });

  it("does not move a note across the pinned boundary", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup([note({ id: "p", title: "Pin", pinned: true }), ...three]);
    await drag(t, 1, 0);
    expect(notesReorder).not.toHaveBeenCalled();
  });

  // Search results span subfolders; arranging that subset would rewrite the
  // order of rows from several levels at once.
  it("hides the grips while searching", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup(three);
    await type(t, "e");
    expect(grips(t)).toHaveLength(0);
  });

  it("lists folders in their arranged order and saves a folder drag", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    folders.set([
      { id: "f1", parent_id: null, name: "Alpha", note_count: 0, sort_order: 1 },
      { id: "f2", parent_id: null, name: "Beta", note_count: 0, sort_order: 0 },
    ]);
    try {
      const t = await setup([]);
      expect(titles(t)).toEqual(["Beta", "Alpha"]);
      await drag(t, 1, 0);
      expect(foldersReorder).toHaveBeenCalledWith(["f1", "f2"]);
    } finally { folders.set([]); }
  });

  it("keeps a stored arrangement when Custom is chosen again", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const t = await setup([
      note({ id: "c", title: "Cee", sort_order: 0 }),
      note({ id: "a", title: "Ay", sort_order: 1 }),
      note({ id: "b", title: "Bee", sort_order: 2 }),
    ]);
    expect(titles(t)).toEqual(["Cee", "Ay", "Bee"]);
  });

  it("leaves an unarranged list stable rather than reshuffling", async () => {
    sortPref.set({ field: "manual", dir: "asc" });
    const list = [note({ id: "b", title: "Bee" }), note({ id: "a", title: "Ay" })];
    const first = titles(await setup(list));
    cleanup?.(); cleanup = null; document.body.innerHTML = "";
    expect(titles(await setup(list))).toEqual(first);
  });
});

describe("relative dates", () => {
  // toLocaleDateString builds a fresh Intl.DateTimeFormat every call. On a list
  // of old notes that is one throwaway formatter per card, per refresh.
  it("formats old notes without building a formatter per card", async () => {
    const spy = vi.spyOn(Date.prototype, "toLocaleDateString");
    const old = Math.floor((Date.now() - 40 * DAY) / 1000);
    const many = Array.from({ length: 25 }, (_, i) =>
      note({ id: `old${i}`, title: `Old ${i}`, updated_at: old }),
    );
    const t = await setup(many);

    expect(spy).not.toHaveBeenCalled();
    // and the date is still rendered, not silently dropped
    expect(t.querySelector(".date")!.textContent!.trim()).not.toBe("");
    spy.mockRestore();
  });

  it("still uses relative units for recent notes", async () => {
    const t = await setup([
      note({ id: "r", title: "Recent", updated_at: Math.floor((Date.now() - 2 * 3_600_000) / 1000) }),
    ]);
    expect(t.querySelector(".date")!.textContent!.trim()).toBe("2h");
  });
});

describe("card ink for background images", () => {
  const BRIGHT = "data:image/png;base64,bright";
  const DARK = "data:image/png;base64,dark";
  let decoded: string[] = [];

  beforeEach(() => {
    decoded = [];
    // happy-dom neither decodes images nor rasterises a canvas, so stand in for
    // both: `bright` reads as a light image (dark ink), `dark` as the opposite.
    class FakeImage {
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      crossOrigin: string | null = null;
      #src = "";
      set src(v: string) {
        this.#src = v;
        decoded.push(v);
        setTimeout(() => this.onload?.(), 0);
      }
      get src() { return this.#src; }
    }
    vi.stubGlobal("Image", FakeImage);
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockImplementation(() => {
      let level = 0;
      return {
        drawImage(img: { src: string }) { level = img.src === BRIGHT ? 230 : 20; },
        getImageData() {
          const d = new Uint8ClampedArray(16 * 16 * 4);
          for (let i = 0; i < d.length; i += 4) {
            d[i] = d[i + 1] = d[i + 2] = level;
            d[i + 3] = 255;
          }
          return { data: d };
        },
      } as unknown as CanvasRenderingContext2D;
    });
  });

  afterEach(() => { vi.unstubAllGlobals(); });

  const card = (t: HTMLElement) => t.querySelector(".note-card")!;

  it("picks dark ink over a light image", async () => {
    bgImages.set({ i1: BRIGHT });
    const t = await setup([note({ id: "i1", title: "Bright" })]);
    await flush();
    expect(card(t).classList.contains("dark-ink")).toBe(true);
  });

  // The bug: the cache was keyed on note id alone, so the very first image a note
  // ever had decided its text colour forever.
  it("re-picks the ink when the note's image changes", async () => {
    bgImages.set({ i1: BRIGHT });
    const t = await setup([note({ id: "i1", title: "Bright" })]);
    await flush();
    expect(card(t).classList.contains("dark-ink")).toBe(true);

    bgImages.set({ i1: DARK });
    await flush();
    await flush();
    expect(card(t).classList.contains("light-ink")).toBe(true);
  });

  it("decodes a shared image once, not once per note", async () => {
    bgImages.set({ a: BRIGHT, b: BRIGHT, c: BRIGHT });
    await setup([note({ id: "a" }), note({ id: "b" }), note({ id: "c" })]);
    await flush();
    expect(decoded.filter(u => u === BRIGHT)).toHaveLength(1);
  });

  it("forgets images that no note uses any more", async () => {
    bgImages.set({ a: BRIGHT });
    await setup([note({ id: "a" })]);
    await flush();
    expect(decoded).toHaveLength(1);

    bgImages.set({});
    await flush();

    bgImages.set({ a: BRIGHT });
    await flush();
    expect(decoded).toHaveLength(2);
  });
});

describe("batch unprotect", () => {
  async function removeProtectionOn(t: HTMLElement, howMany: number) {
    t.querySelector<HTMLButtonElement>(".select-btn")!.click();
    await flush();
    const cards = t.querySelectorAll<HTMLButtonElement>(".note-card");
    for (let i = 0; i < howMany; i++) { cards[i].click(); await flush(); }
    t.querySelector<HTMLButtonElement>('[aria-label="Remove protection"]')!.click();
    await flush();

    const pw = document.querySelector<HTMLInputElement>('.modal input[type="password"]')!;
    pw.value = "hunter2";
    pw.dispatchEvent(new Event("input", { bubbles: true }));
    document.querySelector("form")!.dispatchEvent(
      new Event("submit", { bubbles: true, cancelable: true }),
    );
    await flush();
    await flush();
  }

  it("refreshes the list after a successful batch unprotect", async () => {
    const t = await setup();
    vi.mocked(noteList).mockClear();
    await removeProtectionOn(t, 2);
    expect(notesUnprotect).toHaveBeenCalled();
    expect(noteList).toHaveBeenCalled();
  });

  // The backend unprotects what it can and throws at the end. Skipping the
  // refresh leaves lock icons on notes that are no longer locked.
  it("refreshes the list even when the backend throws part-way", async () => {
    const t = await setup();
    vi.mocked(notesUnprotect).mockRejectedValueOnce(new Error("2 of 3 failed"));
    vi.mocked(noteList).mockClear();
    await removeProtectionOn(t, 2);
    expect(noteList).toHaveBeenCalled();
  });

  it("still shows the error on the modal instead of closing it", async () => {
    const t = await setup();
    vi.mocked(notesUnprotect).mockRejectedValueOnce(new Error("2 of 3 failed"));
    await removeProtectionOn(t, 2);
    expect(document.querySelector(".modal")).toBeTruthy();
    expect(document.querySelector(".modal .error")!.textContent).toContain("2 of 3 failed");
  });
});

describe("trash", () => {
  const binned: TrashedNote[] = [
    { id: "t1", kind: "document", title: "Old draft", has_note_password: false, deleted_at: 1_700_000_000 },
    { id: "t2", kind: "checklist", title: "Vault", has_note_password: true, deleted_at: 1_700_000_500 },
  ];

  async function openTrash(list: TrashedNote[] = binned) {
    vi.mocked(trashList).mockResolvedValue(list);
    listTrash.set(true);
    const t = await setup();
    await flush();
    return t;
  }

  const button = (label: string) =>
    document.querySelector<HTMLButtonElement>(`[aria-label="${label}"]`)!;
  const modalButton = (text: string) =>
    [...document.querySelectorAll<HTMLButtonElement>(".modal button")].find(b => b.textContent?.trim() === text)!;

  it("lists trashed notes with their deleted date instead of the notes", async () => {
    const t = await openTrash();
    expect(titles(t)).toEqual(["Old draft", "Vault"]);
    expect(t.querySelector("input.search")).toBeNull();
    expect(t.querySelectorAll("time").length).toBe(2);
    expect(t.querySelector('[aria-label="Password protected"]')).toBeTruthy();
  });

  it("restores a note and refreshes the list", async () => {
    await openTrash();
    vi.mocked(noteList).mockClear();
    button("Restore Old draft").click();
    await flush();
    expect(trashRestore).toHaveBeenCalledWith(["t1"]);
    expect(noteList).toHaveBeenCalled();
  });

  it("asks before deleting a note forever", async () => {
    await openTrash();
    button("Delete Old draft forever").click();
    await flush();
    expect(trashDelete).not.toHaveBeenCalled();
    modalButton("Delete forever").click();
    await flush();
    expect(trashDelete).toHaveBeenCalledWith(["t1"]);
  });

  it("empties the trash only after confirming", async () => {
    const t = await openTrash();
    t.querySelector<HTMLButtonElement>(".empty-trash")!.click();
    await flush();
    expect(document.querySelector(".modal")!.textContent).toContain("All 2 notes");
    modalButton("Empty trash").click();
    await flush();
    expect(trashEmpty).toHaveBeenCalled();
  });

  it("disables Empty trash when there is nothing in it", async () => {
    const t = await openTrash([]);
    expect(t.querySelector<HTMLButtonElement>(".empty-trash")!.disabled).toBe(true);
    expect(t.textContent).toContain("Trash is empty.");
  });

  it("says a deleted note goes to Trash", async () => {
    const t = await setup();
    t.querySelector<HTMLButtonElement>(".card-menu")!.click();
    await flush();
    [...t.querySelectorAll<HTMLButtonElement>(".popover-item")].find(b => b.textContent?.includes("Delete"))!.click();
    await flush();
    expect(document.querySelector(".modal")!.textContent).toContain("moved to Trash");
  });
});
