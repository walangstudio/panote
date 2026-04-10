import { describe, it, expect, vi, beforeEach } from "vitest";
import { get } from "svelte/store";

vi.mock("$lib/tauri", () => ({ noteList: vi.fn() }));

import { noteList } from "$lib/tauri";
import type { NoteMetadata } from "$lib/tauri";
import { notes, refreshNotes } from "./notes";

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
