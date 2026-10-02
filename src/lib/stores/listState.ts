import { get, writable } from "svelte/store";

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
/// Which folder the list is showing; null means everything. Lives here for the
/// same reason as the search text: opening a note on mobile unmounts the list,
/// and a folder you had drilled into should still be there when you come back.
export const listFolder = writable<string | null>(null);
/// Showing Trash instead of a folder. A flag rather than a sentinel folder id,
/// so no real id can ever collide with it.
export const listTrash = writable(false);

/// Where a new note of `kind` is composed: filed into the folder on screen.
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
}
