import { writable } from "svelte/store";
import type { NoteMetadata } from "$lib/tauri";
import { noteList, noteCount, noteBgImages } from "$lib/tauri";

export const notes = writable<NoteMetadata[]>([]);
/// Total notes in the database. `notes` is capped by the backend page size, so
/// this is what tells the list it is showing a partial view — without it, notes
/// past the cap simply ceased to exist with nothing said.
export const totalNotes = writable(0);

/// Note backgrounds, keyed by note id.
///
/// These are base64 data URIs, measured at ~143x the size of all note bodies
/// combined, and `refreshNotes` runs on every save, pin, delete and incoming
/// transfer. Shipping them with the list meant re-serialising every image
/// through IPC on each of those. They change rarely, so they are fetched
/// separately and only when something could have changed them.
export const bgImages = writable<Record<string, string>>({});

export async function refreshBgImages() {
  try { bgImages.set(await noteBgImages()); } catch { /* keep the cached set */ }
}

export async function refreshNotes(opts: { withBackgrounds?: boolean } = {}) {
  notes.set(await noteList());
  // A failure here must not blank the list; the count is only a disclosure.
  try { totalNotes.set(await noteCount()); } catch { /* leave the last known total */ }
  if (opts.withBackgrounds) await refreshBgImages();
}

export type SortField = "manual" | "updated" | "created" | "title" | "kind";
export type SortDir = "asc" | "desc";
export interface SortPref { field: SortField; dir: SortDir; }

const defaultSort: SortPref = { field: "updated", dir: "desc" };

function loadSort(): SortPref {
  if (typeof window === "undefined") return defaultSort;
  try {
    const saved: SortPref = JSON.parse(localStorage.getItem("panote-sort") ?? "");
    // Custom is withdrawn while its drag does not work. Anyone whose stored
    // preference is already Custom would otherwise open to a list they cannot
    // rearrange and cannot switch away from without finding the sort menu.
    return saved.field === "manual" ? defaultSort : saved;
  } catch { return defaultSort; }
}

export const sortPref = writable<SortPref>(loadSort());
sortPref.subscribe(v => {
  if (typeof window !== "undefined") localStorage.setItem("panote-sort", JSON.stringify(v));
});

// localeCompare builds a collator per call; one shared instance is far cheaper.
const collator = new Intl.Collator();

export function sortNotes(list: NoteMetadata[], pref: SortPref): NoteMetadata[] {
  const primary = (a: NoteMetadata, b: NoteMetadata) => {
    switch (pref.field) {
      case "updated": return a.updated_at - b.updated_at;
      case "created": return a.created_at - b.created_at;
      case "title": return collator.compare(a.title, b.title);
      case "kind": return collator.compare(a.kind, b.kind);
      // Hand-arranged order. Rows never arranged sit at 0 and fall through to the
      // id tiebreak below, so the list stays stable rather than shuffling.
      case "manual": return (a.sort_order ?? 0) - (b.sort_order ?? 0);
    }
  };
  // Ties break on id, and descending negates the comparator instead of reversing
  // the sorted array. Reversing looks equivalent but isn't: sort is stable, so it
  // also flips every tie — and on a field like Kind nearly every pair is a tie, so
  // toggling direction reshuffled each group by nothing more than backend order.
  const dir = pref.dir === "desc" ? -1 : 1;
  const cmp = (a: NoteMetadata, b: NoteMetadata) =>
    (primary(a, b) || collator.compare(a.id, b.id)) * dir;
  // filter() already copies, so the caller's array is never sorted in place.
  return [...list.filter(n => n.pinned).sort(cmp), ...list.filter(n => !n.pinned).sort(cmp)];
}
