import { writable } from "svelte/store";
import type { NoteMetadata } from "$lib/tauri";
import { noteList } from "$lib/tauri";

export const notes = writable<NoteMetadata[]>([]);

export async function refreshNotes() {
  notes.set(await noteList());
}

export type SortField = "updated" | "created" | "title" | "kind";
export type SortDir = "asc" | "desc";
export interface SortPref { field: SortField; dir: SortDir; }

const defaultSort: SortPref = { field: "updated", dir: "desc" };

function loadSort(): SortPref {
  if (typeof window === "undefined") return defaultSort;
  try { return JSON.parse(localStorage.getItem("panote-sort") ?? ""); }
  catch { return defaultSort; }
}

export const sortPref = writable<SortPref>(loadSort());
sortPref.subscribe(v => {
  if (typeof window !== "undefined") localStorage.setItem("panote-sort", JSON.stringify(v));
});

export function sortNotes(list: NoteMetadata[], pref: SortPref): NoteMetadata[] {
  const cmp = (a: NoteMetadata, b: NoteMetadata) => {
    switch (pref.field) {
      case "updated": return a.updated_at - b.updated_at;
      case "created": return a.created_at - b.created_at;
      case "title": return a.title.localeCompare(b.title);
      case "kind": return a.kind.localeCompare(b.kind);
    }
  };
  const apply = (arr: NoteMetadata[]) => {
    const s = [...arr].sort(cmp);
    return pref.dir === "desc" ? s.reverse() : s;
  };
  const pinned = list.filter(n => n.pinned);
  const unpinned = list.filter(n => !n.pinned);
  return [...apply(pinned), ...apply(unpinned)];
}
