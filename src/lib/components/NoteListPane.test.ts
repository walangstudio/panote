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
  noteDelete: vi.fn(async () => {}),
  notePin: vi.fn(async () => {}),
  noteProtect: vi.fn(async () => {}),
  noteUnprotect: vi.fn(async () => {}),
  noteChangePassword: vi.fn(async () => {}),
  notesProtect: vi.fn(async () => {}),
  notesUnprotect: vi.fn(async () => {}),
}));

// The component reads `page.params.id`; the shared stub only carries `page.url`.
import { page } from "$app/state";
(page as unknown as { params: Record<string, string> }).params = { id: "" };

import { noteList, notesUnprotect } from "$lib/tauri";
import { notes, bgImages } from "$lib/stores/notes";
import { resetListState } from "$lib/stores/listState";
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
