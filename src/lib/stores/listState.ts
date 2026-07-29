import { writable } from "svelte/store";

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

/// Clear it. Module-level state outlives any single component by design, which
/// also means it outlives a single test — so tests must reset between cases.
export function resetListState() {
  listFilter.set("");
  listSelecting.set(false);
  listSelected.set(new Set());
}
