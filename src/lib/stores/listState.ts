import { get, writable } from "svelte/store";
import { goto } from "$app/navigation";

/// Note-list UI state that has to outlive the component.
///
/// Below the desktop breakpoint the list is mounted by the route, so opening a
/// note unmounts it — and component-local `$state` went with it. Searching for
/// "invoice", opening a result and pressing back dropped you at the top of an
/// unfiltered list. On desktop the pane lives in the layout and never unmounts,
/// so this changes nothing there.
///
/// In-memory only: a fresh app launch should start with a clear search.
export const listFilter = writable("");
export const listSelecting = writable(false);
export const listSelected = writable(new Set<string>());

/// Drop notes that just went to Trash from the selection, wherever they were
/// trashed from: a batch action must never reach them.
export function forgetSelected(ids: string[]) {
  listSelected.update(s => (ids.some(i => s.has(i)) ? new Set([...s].filter(i => !ids.includes(i))) : s));
}
/// Which folder the list is showing; null means everything. Lives here for the
/// same reason as the search text: opening a note on mobile unmounts the list,
/// and a folder you had drilled into should still be there when you come back.
export const listFolder = writable<string | null>(null);
/// Showing Trash instead of a folder. A flag rather than a sentinel folder id,
/// so no real id can ever collide with it.
export const listTrash = writable(false);
/// Copy / Cut / Paste. Ids and a mode, never content and never the OS
/// clipboard; in-memory only like the rest of this file.
export const listClipboard = writable<{ mode: "copy" | "cut"; kind: "note" | "folder"; ids: string[] } | null>(null);

/// Where a new note of `kind` is composed: filed into the folder on screen.
/// Show folder `id` (null: the root). With a note open this also closes it, so
/// the folder rides in the URL: the editor's unsaved-changes prompt finishes the
/// switch on Save or Discard, and Cancel leaves both the note and the list as
/// they were. The root route applies it (see `applyFolderParam`).
export function openFolder(id: string | null, noteOpen: boolean) {
  if (noteOpen) {
    void goto(`/?folder=${encodeURIComponent(id ?? "")}`);
    return;
  }
  listFolder.set(id);
  listTrash.set(false);
}

/// Show Trash. Leaves an open note the same way `openFolder` does.
export function openTrash(noteOpen: boolean) {
  if (noteOpen) {
    void goto("/?trash=1");
    return;
  }
  listTrash.set(true);
}

/// The other half of `openFolder` and `openTrash`: run by the root route on arrival.
export function applyFolderParam(url: URL): boolean {
  if (url.searchParams.has("trash")) {
    listTrash.set(true);
    return true;
  }
  const folder = url.searchParams.get("folder");
  if (folder === null) return false;
  listFolder.set(folder || null);
  listTrash.set(false);
  return true;
}

export const newNoteHref = (kind: string) => {
  const folder = get(listFolder);
  return `/note/new?kind=${kind}${folder ? `&folder=${encodeURIComponent(folder)}` : ""}`;
};

/// Clear it. Module-level state outlives any single component by design, which
/// also means it outlives a single test — so tests must reset between cases.
export function resetListState() {
  listFilter.set("");
  listSelecting.set(false);
  listSelected.set(new Set());
  listFolder.set(null);
  listTrash.set(false);
  listClipboard.set(null);
}
