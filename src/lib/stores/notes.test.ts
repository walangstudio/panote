import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";

vi.mock("$lib/tauri", () => ({ noteList: vi.fn() }));

import { noteList } from "$lib/tauri";
import type { NoteMetadata } from "$lib/tauri";
import { notes, refreshNotes, sortNotes } from "./notes";

const fixture: NoteMetadata[] = [
  {
    id: "abc",
    kind: "document",
    title: "Test note",
    tags: ["a"],
    created_at: 1000,
    updated_at: 2000,
    has_note_password: false,
    pinned: false,
    show_preview: true,
  },
];

describe("notes store", () => {
  beforeEach(() => {
    notes.set([]);
    vi.clearAllMocks();
  });

  it("starts empty", () => {
    expect(get(notes)).toEqual([]);
  });

  it("refreshNotes populates the store", async () => {
    vi.mocked(noteList).mockResolvedValue(fixture);
    await refreshNotes();
    expect(get(notes)).toEqual(fixture);
  });

  it("refreshNotes replaces previous contents", async () => {
    vi.mocked(noteList).mockResolvedValue(fixture);
    await refreshNotes();
    vi.mocked(noteList).mockResolvedValue([]);
    await refreshNotes();
    expect(get(notes)).toEqual([]);
  });

  it("refreshNotes calls noteList once per invocation", async () => {
    vi.mocked(noteList).mockResolvedValue([]);
    await refreshNotes();
    await refreshNotes();
    expect(noteList).toHaveBeenCalledTimes(2);
  });
});

// Sorting by a low-cardinality field (Kind) makes almost every pair a tie, so how
// ties are settled *is* the visible order. It has to come from the notes, never
// from the order the backend happened to hand them over in.
describe("sortNotes", () => {
  const note = (over: Partial<NoteMetadata> & { id: string }): NoteMetadata => ({
    kind: "document",
    title: "T",
    tags: [],
    created_at: 0,
    updated_at: 0,
    has_note_password: false,
    pinned: false,
    show_preview: true,
    ...over,
  });

  const ids = (list: NoteMetadata[]) => list.map(n => n.id);

  // Arrival order (b, a, c) deliberately disagrees with id order so a tie-break
  // that leans on arrival order produces a different answer than one that doesn't.
  const a = note({ id: "a", kind: "document" });
  const b = note({ id: "b", kind: "document" });
  const c = note({ id: "c", kind: "checklist" });
  const arrival = [b, a, c];

  it("breaks ties on id, not on the order the backend returned", () => {
    expect(ids(sortNotes(arrival, { field: "kind", dir: "asc" }))).toEqual(["c", "a", "b"]);
  });

  it("inverting direction inverts the whole order instead of reshuffling ties", () => {
    expect(ids(sortNotes(arrival, { field: "kind", dir: "desc" }))).toEqual(["b", "a", "c"]);
  });

  it("gives the same order whatever order the notes arrive in", () => {
    for (const dir of ["asc", "desc"] as const) {
      const fromArrival = ids(sortNotes(arrival, { field: "kind", dir }));
      const fromShuffle = ids(sortNotes([c, a, b], { field: "kind", dir }));
      expect(fromShuffle).toEqual(fromArrival);
    }
  });

  it("settles equal timestamps deterministically too", () => {
    const x = note({ id: "x", updated_at: 5 });
    const y = note({ id: "y", updated_at: 5 });
    expect(ids(sortNotes([y, x], { field: "updated", dir: "desc" }))).toEqual(
      ids(sortNotes([x, y], { field: "updated", dir: "desc" })),
    );
  });

  it("keeps pinned notes ahead of the rest in both directions", () => {
    const p = note({ id: "p", kind: "table", pinned: true });
    for (const dir of ["asc", "desc"] as const) {
      expect(ids(sortNotes([b, p, c], { field: "kind", dir }))[0]).toBe("p");
    }
  });

  // The default direction is descending. Honouring it here showed every drop
  // upside down: the drag saved b,c,a and the list painted a,c,b.
  it("keeps Custom in the order it was arranged, whatever the direction", () => {
    const list = [note({ id: "x", sort_order: 2 }), note({ id: "y", sort_order: 0 }), note({ id: "z", sort_order: 1 })];
    for (const dir of ["asc", "desc"] as const) {
      expect(ids(sortNotes(list, { field: "manual", dir }))).toEqual(["y", "z", "x"]);
    }
  });

  it("does not mutate the array it was given", () => {
    const input = [...arrival];
    sortNotes(input, { field: "kind", dir: "desc" });
    expect(ids(input)).toEqual(["b", "a", "c"]);
  });
});
