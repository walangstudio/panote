<script lang="ts">
  import { onMount, tick } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import {
    notes, totalNotes, bgImages, refreshNotes, sortPref, sortNotes, type SortField,
    trashed, refreshTrash,
  } from "$lib/stores/notes";
  import {
    notesDelete, notePin, trashRestore, trashDelete, trashEmpty,
    noteProtect, noteUnprotect, noteChangePassword, notesProtect, notesUnprotect,
  } from "$lib/tauri";
  import type { NoteMetadata } from "$lib/tauri";
  import { get } from "svelte/store";
  import { sidebarOpen } from "$lib/stores/sidebar";
  import { listFilter, listSelecting, listSelected, listFolder, listTrash, listClipboard, newNoteHref, openFolder } from "$lib/stores/listState";
  import { folders, refreshFolders } from "$lib/stores/folders";
  import {
    folderCreate, folderMove, folderRename, folderDelete, noteSetFolder,
    notesReorder, foldersReorder, notesCopy, folderCopy,
  } from "$lib/tauri";
  import FolderPickerModal from "$lib/components/FolderPickerModal.svelte";
  import FolderNameModal from "$lib/components/FolderNameModal.svelte";
  import ConfirmModal from "$lib/components/ConfirmModal.svelte";
  import PasswordModal from "$lib/components/PasswordModal.svelte";
  import NewNoteModal from "$lib/components/NewNoteModal.svelte";
  import { showMenu, anchorMenu, type MenuAction, type Rect } from "$lib/contextMenu";
  import { shortcutFor, findBelongsToNote } from "$lib/shortcuts";

  interface Props { desktop?: boolean; }
  let { desktop = false }: Props = $props();

  // Seeded from the stores so a round trip to a note and back does not wipe the
  // search or the selection, then written back as they change. On desktop the
  // pane never unmounts, so this is a no-op there.
  let filter = $state(get(listFilter));
  let selecting = $state(get(listSelecting));
  let selected = $state(get(listSelected));

  $effect(() => { listFilter.set(filter); });
  $effect(() => { listSelecting.set(selecting); });
  $effect(() => { listSelected.set(selected); });
  let transferNoteIds = $state<string[] | null>(null);
  let deleteTargets = $state<string[] | null>(null);
  let sortOpen = $state(false);
  /// The in-app menu, shown only where a native one could not be.
  let menu = $state<{ anchor: Rect; actions: MenuAction[] } | null>(null);
  let searchInput: HTMLInputElement | undefined = $state();
  let fabOpen = $state(false);
  let showNewNote = $state(false);

  // ---- Folder create / move ----
  //
  // The backend is what actually protects the tree: folder_move refuses a cycle
  // or a move past the depth cap. These surface its refusal rather than
  // duplicating the rule, so the two can never disagree.
  let nameModal = $state<{ mode: "create" | "rename"; id?: string; initial: string } | null>(null);
  let nameError = $state("");
  /// What is being moved: a note, or a folder (whose own subtree is off-limits).
  let moveTarget = $state<{ kind: "note" | "folder"; id: string; from: string | null } | null>(null);
  let moveError = $state("");

  async function submitName(name: string) {
    if (!nameModal) return;
    nameError = "";
    try {
      if (nameModal.mode === "create") await folderCreate(name, $listFolder);
      else await folderRename(nameModal.id!, name);
      await refreshFolders();
      nameModal = null;
    } catch (e) {
      nameError = folderMessage(e);
    }
  }

  async function submitMove(dest: string | null) {
    if (!moveTarget) return;
    moveError = "";
    try {
      if (moveTarget.kind === "note") await noteSetFolder(moveTarget.id, dest);
      else await folderMove(moveTarget.id, dest);
      await Promise.all([refreshFolders(), refreshNotes()]);
      moveTarget = null;
    } catch (e) {
      moveError = folderMessage(e);
    }
  }

  /// Turn the backend's codes into something a person can act on.
  function folderMessage(e: unknown): string {
    const raw = String(e);
    if (raw.includes("FOLDER_CYCLE")) return "A folder can't be moved inside itself.";
    if (raw.includes("FOLDER_TOO_DEEP")) return "That would nest folders too deeply.";
    if (raw.includes("FOLDER_NAME_EMPTY")) return "Give the folder a name.";
    return raw;
  }

  async function removeFolder(id: string) {
    try {
      await folderDelete(id);
      await Promise.all([refreshFolders(), refreshNotes()]);
      if ($listFolder === id) listFolder.set(null);
    } catch (e) { moveError = folderMessage(e); }
  }

  // ---- Clipboard ----
  //
  // Paste always lands in the folder on screen, or in the folder whose menu
  // says "Paste into". Cut moves with the same commands as "Move to"; copy
  // duplicates in the backend, which re-seals protected notes for their new ids.

  function clip(mode: "copy" | "cut", kind: "note" | "folder", ids: string[]) {
    listClipboard.set({ mode, kind, ids });
    if (selecting) { selecting = false; selected = new Set(); }
  }

  async function paste(dest: string | null) {
    const c = $listClipboard;
    if (!c) return;
    moveError = "";
    try {
      if (c.mode === "cut") {
        for (const id of c.ids) await (c.kind === "note" ? noteSetFolder(id, dest) : folderMove(id, dest));
        listClipboard.set(null);
      } else {
        let skipped = 0;
        if (c.kind === "note") skipped = (await notesCopy(c.ids, dest)).skipped_locked;
        else for (const id of c.ids) skipped += (await folderCopy(id, dest)).skipped_locked;
        if (skipped) moveError = skipped === 1
          ? "A locked note was skipped. Unlock it first to copy."
          : `${skipped} locked notes were skipped. Unlock them first to copy.`;
      }
    } catch (e) {
      moveError = folderMessage(e);
    }
    await Promise.all([refreshFolders(), refreshNotes()]);
  }

  const isCut = (id: string) => $listClipboard?.mode === "cut" && $listClipboard.ids.includes(id);

  const activeId = $derived(page.params.id ?? "");

  type PwMode = "set" | "change" | "remove";
  let pwModal = $state<{ mode: PwMode; ids: string[]; isBatch: boolean } | null>(null);

  // Only batch set/remove override the modal's default heading; everything else
  // returns undefined so PasswordModal falls back to its own per-mode title.
  const pwHeading = $derived(
    pwModal?.isBatch && pwModal.mode !== "change"
      ? pwModal.mode === "set"
        ? `Protect ${pwModal.ids.length} notes`
        : `Remove protection from ${pwModal.ids.length} notes`
      : undefined,
  );

  async function handlePassword(v: { password: string; oldPassword?: string }) {
    if (!pwModal) return;
    const { mode, ids, isBatch } = pwModal;
    try {
      if (mode === "set") {
        if (isBatch) await notesProtect(ids, v.password);
        else await noteProtect(ids[0], v.password);
      } else if (mode === "change") {
        await noteChangePassword(ids[0], v.oldPassword ?? "", v.password);
      } else {
        if (isBatch) await notesUnprotect(ids, v.password);
        else await noteUnprotect(ids[0], v.password);
      }
    } finally {
      // A batch call reports failure only after it has already changed some
      // notes, so the list is stale whether or not it threw. Refresh either way;
      // the error keeps propagating, so the modal still shows what went wrong.
      await refreshNotes();
      if (isBatch) { selecting = false; selected = new Set(); }
    }
  }

  // Bottom-up speed-dial: last item sits nearest the FAB (prototype order).
  const fabKinds = [
    { id: "folder", icon: "create_new_folder", label: "Folder" },
    { id: "table", icon: "table_chart", label: "Table" },
    { id: "kanban", icon: "view_kanban", label: "Kanban" },
    { id: "checklist", icon: "checklist", label: "Checklist" },
    { id: "document", icon: "edit_note", label: "Document" },
  ] as const;

  // ---- Custom arrangement ----
  //
  // Live whenever Custom is the active sort - no mode to enter first. Under any
  // other sort a drag would appear to work and then be undone by the next
  // refresh, so the grips simply are not there. Nor while searching: the
  // results are a subset spanning subfolders, not one level to arrange.
  const reordering = $derived($sortPref.field === "manual" && !filter.trim());

  /// Set once a drag actually moves something. Rows stay clickable in Custom
  /// view, so only a real drag may swallow the click that follows it.
  let didDrag = $state(false);

  /// Local order while dragging, so rows follow the finger without a round trip.
  let dragIds = $state<string[] | null>(null);
  let dragKind = $state<"note" | "folder" | null>(null);
  let dragId = $state<string | null>(null);

  function startReorder(e: PointerEvent, kind: "note" | "folder", id: string, ids: string[]) {
    e.preventDefault();
    e.stopPropagation();
    // No setPointerCapture: reordering moves the grip's DOM node, and a moved
    // node loses its capture, which killed the drag part-way. Window listeners
    // are what the kanban board uses for the same job and they survive it.
    didDrag = false;
    dragKind = kind;
    dragId = id;
    dragIds = [...ids];
    window.addEventListener("pointermove", onReorderMove);
    window.addEventListener("pointerup", onReorderUp);
    window.addEventListener("pointercancel", onReorderUp);
  }

  /// `ids` with `id` moved to index `to`; unchanged if either is out of range.
  function moved(ids: string[], id: string, to: number): string[] {
    const from = ids.indexOf(id);
    if (from === -1 || to < 0 || to >= ids.length) return ids;
    const next = [...ids];
    next.splice(to, 0, ...next.splice(from, 1));
    return next;
  }

  function onReorderMove(e: PointerEvent) {
    if (!dragIds || !dragId) return;
    // Hit-test the row under the finger, the same approach the kanban board uses,
    // so this works with a mouse and a touch without separate code paths.
    let el = document.elementFromPoint(e.clientX, e.clientY) as HTMLElement | null;
    while (el) {
      const over = el.dataset?.rowId;
      if (over && over !== dragId) {
        if (dragIds.includes(over)) {
          dragIds = moved(dragIds, dragId, dragIds.indexOf(over));
          didDrag = true;
        }
        return;
      }
      el = el.parentElement;
    }
  }

  async function onReorderUp() {
    window.removeEventListener("pointermove", onReorderMove);
    window.removeEventListener("pointerup", onReorderUp);
    window.removeEventListener("pointercancel", onReorderUp);
    const ids = dragIds;
    const kind = dragKind;
    dragIds = null; dragKind = null; dragId = null;
    if (!ids || !kind || !didDrag) return;
    // Cleared after the click that ends the drag has had its chance to fire.
    setTimeout(() => { didDrag = false; }, 0);
    await saveOrder(kind, ids);
  }

  async function saveOrder(kind: "note" | "folder", ids: string[]): Promise<boolean> {
    try {
      if (kind === "note") await notesReorder(ids);
      else await foldersReorder(ids);
      await Promise.all([refreshNotes(), refreshFolders()]);
      return true;
    } catch (e) { moveError = folderMessage(e); return false; }
  }

  /// Arrow keys on a focused grip: the keyboard route for what the drag does.
  let reorderAnnouncement = $state("");
  async function reorderByKey(e: KeyboardEvent, kind: "note" | "folder", id: string, label: string, ids: string[]) {
    e.stopPropagation();
    const step = e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1 : 0;
    if (!step) return;
    e.preventDefault();
    const next = moved(ids, id, ids.indexOf(id) + step);
    if (next === ids) return;
    if (!(await saveOrder(kind, next))) return;
    reorderAnnouncement = `${label} moved to position ${next.indexOf(id) + 1} of ${next.length}`;
    await tick();
    // The keyed row moved, and a moved node drops focus.
    document.querySelector<HTMLElement>(`[data-row-id="${CSS.escape(id)}"] .drag-grip`)?.focus();
  }

  /// Rows in the order to paint: the in-flight arrangement for the group holding
  /// the dragged row, otherwise whatever the backend gave us.
  ///
  /// Derived rather than a function call in the template - a call did not track
  /// `dragIds` as a dependency, so the list only reordered once the drag ended
  /// and the rows never followed the pointer.
  function arrange<T extends { id: string }>(rows: T[], ids: string[] | null): T[] {
    if (!ids || !rows.some(r => r.id === dragId)) return rows;
    const by = new Map(rows.map(r => [r.id, r]));
    return ids.map(id => by.get(id)).filter((r): r is T => !!r);
  }

  const sortOptions: { field: SortField; label: string }[] = [
    { field: "manual", label: "Custom" },
    { field: "updated", label: "Date edited" },
    { field: "created", label: "Date created" },
    { field: "title", label: "Title" },
    { field: "kind", label: "Kind" },
  ];

  // Backgrounds are fetched here and then only when something could have
  // changed them — not on every pin, delete or save.
  onMount(() => { refreshNotes({ withBackgrounds: true }); });

  // ponytail: defense in depth — backend already validates bg_image is a data:image/... URI.
  function safeBgImageUrl(bgImage: string | null | undefined): string | undefined {
    return bgImage && bgImage.startsWith("data:image/") ? `url(${bgImage})` : undefined;
  }

  // Sorting is O(n log n) and only depends on the notes + pref, so it must not
  // rerun on every keystroke — filtering the sorted result is the cheap part.
  const sorted = $derived(sortNotes($notes, $sortPref));
  // Normalised once per keystroke, not once per note per keystroke.
  const query = $derived(filter.trim().toLowerCase());
  // preview_text is printed on the card, so a phrase the user can read off the
  // screen has to be findable too.
  const matches = (n: NoteMetadata) =>
    n.title.toLowerCase().includes(query) ||
    n.tags.some(t => t.toLowerCase().includes(query)) ||
    (n.preview_text?.toLowerCase().includes(query) ?? false);
  // Explorer semantics: you see what is *directly* inside where you are — the
  // subfolders and the notes — and you go one level at a time. The root is the
  // top-level folders plus everything not filed anywhere.
  const childFolders = $derived(
    $folders
      .filter(f => (f.parent_id ?? null) === $listFolder)
      .sort((a, b) =>
        ($sortPref.field === "manual" ? (a.sort_order ?? 0) - (b.sort_order ?? 0) : 0) ||
        a.name.localeCompare(b.name)),
  );
  /// Direct children only, so the list matches the folder row you clicked.
  const directNotes = $derived(sorted.filter(n => (n.folder_id ?? null) === $listFolder));

  /// Every folder id at or below `id`, for the search-spans-subfolders case.
  function subtreeIds(id: string | null): Set<string> {
    const out = new Set<string>();
    const walk = (parent: string | null, depth: number) => {
      if (depth > 20) return;
      for (const f of $folders) {
        if ((f.parent_id ?? null) === parent) { out.add(f.id); walk(f.id, depth + 1); }
      }
    };
    if (id !== null) out.add(id);
    walk(id, 0);
    return out;
  }

  // Searching looks into subfolders, the way Explorer does — a search that
  // stopped at the current level would hide the thing you are looking for.
  const searchScope = $derived(() => {
    if ($listFolder === null) return sorted;
    const ids = subtreeIds($listFolder);
    return sorted.filter(n => n.folder_id && ids.has(n.folder_id));
  });
  const inFolder = $derived(query ? searchScope() : directNotes);

  /// Root-to-here, for the breadcrumb.
  const trail = $derived(() => {
    const out: { id: string; name: string }[] = [];
    let cursor = $listFolder;
    for (let i = 0; cursor && i <= 20; i++) {
      const f = $folders.find(x => x.id === cursor);
      if (!f) break;
      out.unshift({ id: f.id, name: f.name });
      cursor = f.parent_id ?? null;
    }
    return out;
  });

  /// How many things sit directly inside a folder — which is exactly what
  /// opening it will show, so the number never disagrees with the list.
  function directCount(id: string): number {
    const subs = $folders.filter(f => (f.parent_id ?? null) === id).length;
    return subs + $notes.filter(n => (n.folder_id ?? null) === id).length;
  }
  const filtered = $derived(
    // No query → show everything, including secret notes. With one, secret notes
    // are never searchable — not by title, tags, preview, or anything.
    query ? inFolder.filter(n => !n.has_note_password && matches(n)) : inFolder,
  );
  // Holding locked notes back is deliberate, but silently returning nothing reads
  // as broken search, so the list owns up to what it withheld.
  const hiddenLocked = $derived(
    query ? inFolder.filter(n => n.has_note_password).length : 0,
  );

  /// How many notes the backend page cap is leaving out. Only meaningful with no
  /// search active — with a query the list is deliberately a subset.
  const notShown = $derived(query ? 0 : Math.max(0, $totalNotes - $notes.length));

  function doDelete(id: string) {
    deleteTargets = [id];
  }

  async function confirmDelete() {
    if (!deleteTargets) return;
    const ids = deleteTargets;
    deleteTargets = null;
    try {
      await notesDelete(ids);
    } catch (e) {
      moveError = String(e);
      return;
    } finally {
      if (ids.length > 1) { selecting = false; selected = new Set(); }
      await Promise.all([refreshNotes(), refreshFolders()]);
    }
    // The deleted note may be the one open in the detail pane.
    if (desktop && ids.includes(activeId)) goto("/");
  }

  // ---- Row menus ----
  //
  // One action list per row kind, rendered by the native menu on desktop and by
  // the in-app popover everywhere else.

  function noteActions(note: NoteMetadata): MenuAction[] {
    const pw = (mode: PwMode) => () => { pwModal = { mode, ids: [note.id], isBatch: false }; };
    return [
      { label: note.pinned ? "Unpin" : "Pin", icon: "push_pin", run: () => togglePin(note.id, note.pinned) },
      { label: "View", icon: "visibility", run: () => goto(`/note/${note.id}?mode=view`) },
      { label: "Move to", icon: "swap_horiz", run: () => { moveError = ""; moveTarget = { kind: "note", id: note.id, from: note.folder_id ?? null }; } },
      { label: "Copy", icon: "content_copy", run: () => clip("copy", "note", [note.id]) },
      { label: "Cut", icon: "content_cut", run: () => clip("cut", "note", [note.id]) },
      ...($listClipboard ? [{ label: "Paste here", icon: "content_paste", run: () => paste($listFolder) }] : []),
      { label: "Edit", icon: "edit", run: () => goto(`/note/${note.id}?mode=edit`) },
      ...(note.has_note_password
        ? [
            { label: "Change password", icon: "password", run: pw("change") },
            { label: "Remove password", icon: "lock_open", danger: true, run: pw("remove") },
          ]
        : [{ label: "Set password", icon: "lock", run: pw("set") }]),
      { label: "Delete", icon: "delete", danger: true, run: () => doDelete(note.id) },
    ];
  }

  function folderActions(f: { id: string; name: string; parent_id?: string | null }): MenuAction[] {
    return [
      { label: "New note inside", icon: "note_add", run: () => { listFolder.set(f.id); showNewNote = true; } },
      { label: "New folder inside", icon: "create_new_folder", run: () => { listFolder.set(f.id); nameModal = { mode: "create", initial: "" }; } },
      { label: "Rename", icon: "edit", run: () => { nameError = ""; nameModal = { mode: "rename", id: f.id, initial: f.name }; } },
      { label: "Move to", icon: "swap_horiz", run: () => { moveError = ""; moveTarget = { kind: "folder", id: f.id, from: f.parent_id ?? null }; } },
      { label: "Copy", icon: "content_copy", run: () => clip("copy", "folder", [f.id]) },
      { label: "Cut", icon: "content_cut", run: () => clip("cut", "folder", [f.id]) },
      ...($listClipboard ? [{ label: "Paste into", icon: "content_paste", run: () => paste(f.id) }] : []),
      { label: "Delete", icon: "delete", danger: true, run: () => removeFolder(f.id) },
    ];
  }

  /// Kebab click or right-click. The fallback popover opens at the cursor for a
  /// right-click and at the button otherwise.
  function openMenu(e: MouseEvent, actions: MenuAction[]) {
    // A long-press on touch fires contextmenu too; the kebab is the way in there.
    if (e.type === "contextmenu" && window.matchMedia?.("(hover: none)").matches) return;
    e.preventDefault();
    e.stopPropagation();
    // A keyboard-raised contextmenu has no pointer position.
    const atCursor = e.type === "contextmenu" && (e.clientX || e.clientY);
    const anchor: Rect = atCursor
      ? { left: e.clientX, right: e.clientX, top: e.clientY, bottom: e.clientY }
      : (e.currentTarget as HTMLElement).getBoundingClientRect();
    showMenu(actions, () => { menu = { anchor, actions }; });
  }

  function onShortcut(e: KeyboardEvent) {
    const s = shortcutFor(e);
    if (!s) return;
    if (s === "escape") {
      if (menu || sortOpen || fabOpen) { e.preventDefault(); menu = null; sortOpen = false; fabOpen = false; }
      else if ($listClipboard && !e.defaultPrevented && !document.querySelector('[aria-modal="true"]')) listClipboard.set(null);
      return;
    }
    // An open dialog owns the keyboard, and Trash has nothing to create or search.
    if ($listTrash || document.querySelector('[aria-modal="true"]')) return;
    // The open note's find bar takes it instead.
    if (s === "find" && findBelongsToNote(e.target)) return;
    e.preventDefault();
    // The selection, else the row with keyboard focus, else the note open beside
    // the list: a focused row is what the user is pointing at.
    const row = (e.target as HTMLElement | null)?.closest?.<HTMLElement>("[data-row-id]");
    const focusedFolder = row?.classList.contains("folder-card") ? row.dataset.rowId : undefined;
    const ids = selecting && selected.size ? [...selected]
      : row && !focusedFolder ? [row.dataset.rowId!]
      : focusedFolder ? []
      : desktop && activeId && activeId !== "new" ? [activeId] : [];
    if (s === "new-note") showNewNote = true;
    else if (s === "new-folder") { nameError = ""; nameModal = { mode: "create", initial: "" }; }
    else if (s === "find") searchInput?.focus();
    else if (s === "copy" || s === "cut") {
      if (ids.length) clip(s, "note", ids);
      else if (focusedFolder) clip(s, "folder", [focusedFolder]);
    }
    else if (s === "paste") paste($listFolder);
    // A focused folder is left alone: deleting one has no confirm step yet.
    else if (ids.length) deleteTargets = ids;
  }

  // ---- Trash ----

  $effect(() => { if ($listTrash) refreshTrash(); });

  /// What the confirm dialog is about to destroy for good.
  let purgeTarget = $state<{ ids: string[]; title: string } | "all" | null>(null);
  let trashHeading: HTMLElement | undefined = $state();
  let trashListEl: HTMLElement | undefined = $state();

  /// The row whose button had focus is gone; land on the next one rather than
  /// dropping keyboard focus to the page.
  async function afterTrashChange() {
    await Promise.all([refreshTrash(), refreshNotes(), refreshFolders()]);
    await tick();
    (trashListEl?.querySelector<HTMLElement>("[data-restore]") ?? trashHeading)?.focus();
  }

  async function restore(id: string) {
    await trashRestore([id]);
    await afterTrashChange();
  }

  async function confirmPurge() {
    const target = purgeTarget;
    purgeTarget = null;
    if (!target) return;
    if (target === "all") await trashEmpty();
    else await trashDelete(target.ids);
    await afterTrashChange();
  }

  const deletedFormat = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" });

  async function togglePin(id: string, currentPinned: boolean) {
    await notePin(id, !currentPinned);
    await refreshNotes();
  }

  function toggleSelect() {
    selecting = !selecting;
    if (!selecting) selected = new Set();
  }

  function toggleNote(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id); else next.add(id);
    selected = next;
  }

  function sendSelected() {
    transferNoteIds = Array.from(selected);
  }

  const kindIcon: Record<string, string> = {
    checklist: "checklist", kanban: "view_kanban", table: "table_chart",
  };
  const kindColor: Record<string, string> = {
    checklist: "tertiary", kanban: "tertiary", table: "secondary",
  };
  const hintIcon: Record<string, string> = {
    plain: "description", markdown: "edit_note", code: "code",
  };
  const hintColor: Record<string, string> = {
    plain: "accent", markdown: "secondary", code: "secondary",
  };

  function noteIcon(note: { kind: string; content_hint?: string }) {
    if (note.kind === "document") return hintIcon[note.content_hint ?? ""] ?? "edit_note";
    return kindIcon[note.kind] ?? "description";
  }
  function noteColor(note: { kind: string; content_hint?: string }) {
    if (note.kind === "document") return hintColor[note.content_hint ?? ""] ?? "accent";
    return kindColor[note.kind] ?? "accent";
  }

  // toLocaleDateString builds a throwaway Intl.DateTimeFormat per call, and this
  // runs for every note older than a week on every refresh — same reasoning as
  // the shared Intl.Collator in stores/notes.ts.
  const dateFormat = new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric" });

  function formatRelative(unixSecs: number) {
    const HOUR = 3_600_000, DAY = 24 * HOUR;
    const diff = Date.now() - unixSecs * 1000;
    if (diff < HOUR) return Math.max(1, Math.round(diff / 60_000)) + "m";
    if (diff < DAY) return Math.round(diff / HOUR) + "h";
    if (diff < 7 * DAY) return Math.round(diff / DAY) + "d";
    return dateFormat.format(new Date(unixSecs * 1000));
  }

  const selectionBar = $derived(selecting && selected.size > 0 && !$listTrash);
  const clipboardBar = $derived(!!$listClipboard && !$listTrash);

  const pinnedNotes = $derived(filtered.filter(n => n.pinned));
  const otherNotes = $derived(filtered.filter(n => !n.pinned));
  const orderedPinned = $derived(arrange(pinnedNotes, dragIds));
  const orderedNotes = $derived(arrange(otherNotes, dragIds));
  const orderedFolders = $derived(arrange(childFolders, dragIds));


  // Pick dark vs light ink for a note's custom background so text stays legible.
  function isLightColor(c: string): boolean {
    const hex = c.trim().match(/^#?([0-9a-f]{3}|[0-9a-f]{6})$/i);
    let r: number, g: number, b: number;
    if (hex) {
      let h = hex[1];
      if (h.length === 3) h = h.split("").map(x => x + x).join("");
      r = parseInt(h.slice(0, 2), 16); g = parseInt(h.slice(2, 4), 16); b = parseInt(h.slice(4, 6), 16);
    } else {
      const m = c.match(/(\d+)[,\s]+(\d+)[,\s]+(\d+)/);
      if (!m) return true;
      r = +m[1]; g = +m[2]; b = +m[3];
    }
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.6;
  }

  // Keyed on the image itself, never the note id: a note that swaps its
  // background needs a fresh reading, and notes sharing an image cost one decode.
  let imgInk = $state<Record<string, "dark" | "light">>({});
  const analyzing = new Set<string>();

  // Results land one image at a time but are published together. Spreading the
  // map per image is O(k^2) allocation across k images, and every write
  // invalidates each card's ink, so a library of backgrounds re-rendered the
  // whole list once per image while it loaded.
  let settled: Record<string, "dark" | "light"> = {};
  let flushQueued = false;
  function record(url: string, ink: "dark" | "light") {
    settled[url] = ink;
    if (flushQueued) return;
    flushQueued = true;
    queueMicrotask(() => {
      flushQueued = false;
      imgInk = { ...imgInk, ...settled };
      settled = {};
    });
  }

  function analyzeImage(url: string) {
    const img = new Image();
    img.crossOrigin = "anonymous";
    img.onload = () => {
      try {
        const c = document.createElement("canvas");
        c.width = 16; c.height = 16;
        const ctx = c.getContext("2d")!;
        ctx.drawImage(img, 0, 0, 16, 16);
        const d = ctx.getImageData(0, 0, 16, 16).data;
        let sum = 0;
        for (let i = 0; i < d.length; i += 4) sum += 0.299 * d[i] + 0.587 * d[i + 1] + 0.114 * d[i + 2];
        record(url, sum / (d.length / 4) / 255 > 0.6 ? "dark" : "light");
      } catch { record(url, "dark"); }
    };
    img.onerror = () => record(url, "dark");
    img.src = url;
  }

  /// Backgrounds no longer ride in the list payload, so a note's image comes
  /// from the separately-cached map instead of the row.
  const bgOf = (id: string): string | undefined => $bgImages[id];

  $effect(() => {
    const live = new Set(Object.values($bgImages).filter((u): u is string => !!u));
    for (const url of live) {
      if (url in imgInk || analyzing.has(url)) continue;
      analyzing.add(url);
      analyzeImage(url);
    }
    // Forget images no note carries any more, so the cache tracks the library
    // instead of growing for the life of the session.
    const stale = Object.keys(imgInk).filter(u => !live.has(u));
    if (stale.length) {
      for (const u of stale) analyzing.delete(u);
      imgInk = Object.fromEntries(Object.entries(imgInk).filter(([u]) => live.has(u)));
    }
  });

  function cardInk(note: { id: string; bg_color?: string }): "dark" | "light" | null {
    if (note.bg_color) return isLightColor(note.bg_color) ? "dark" : "light";
    const url = bgOf(note.id);
    if (url) return imgInk[url] ?? "dark";
    return null;
  }
</script>

<svelte:window onkeydown={onShortcut} />

<div class="page" class:desktop>
  <div class="list-head">
  {#if desktop}
    <div class="pane-head">
      <button class="menu-btn" onclick={() => $sidebarOpen = true} aria-label="Open menu">
        <span class="material-symbols-outlined">menu</span>
      </button>
      <span class="wordmark">Panote</span>
      <button class="compose-btn" onclick={() => showNewNote = true} aria-label="New note">
        <span class="material-symbols-outlined">add</span>
      </button>
    </div>
  {/if}
  <div class="toolbar">
    {#if !desktop}
      <button class="menu-btn" onclick={() => $sidebarOpen = true} aria-label="Open menu">
        <span class="material-symbols-outlined">menu</span>
      </button>
    {/if}
    {#if $listTrash}
      <h2 class="trash-title" tabindex="-1" bind:this={trashHeading}>Trash</h2>
      <button class="empty-trash" disabled={!$trashed.length} onclick={() => purgeTarget = "all"}>Empty trash</button>
    {:else}
    <div class="search-wrap">
      <span class="material-symbols-outlined search-icon">search</span>
      <input class="search" placeholder="Search notes" bind:value={filter} bind:this={searchInput} />
      {#if filter}
        <button class="clear-btn" onclick={() => filter = ""} aria-label="Clear search">
          <span class="material-symbols-outlined" style="font-size: 18px;">close</span>
        </button>
      {/if}
    </div>
    <div class="sort-wrap">
      <button class="sort-btn" onclick={() => sortOpen = !sortOpen} aria-label="Sort notes">
        <span class="material-symbols-outlined">swap_vert</span>
      </button>
      {#if sortOpen}
        <div class="sort-backdrop" role="presentation" onclick={() => sortOpen = false}></div>
        <div class="sort-dropdown">
          {#each sortOptions as opt}
            <button class="sort-option" class:active={$sortPref.field === opt.field}
              onclick={() => sortPref.update(c => ({ field: opt.field, dir: c.dir }))}>
              <span class="material-symbols-outlined" style="font-size: 18px;">{$sortPref.field === opt.field ? "radio_button_checked" : "radio_button_unchecked"}</span>
              <span>{opt.label}</span>
            </button>
          {/each}
          {#if $sortPref.field !== "manual"}
            <div class="sort-divider"></div>
            <button class="sort-option"
              onclick={() => sortPref.update(c => ({ field: c.field, dir: c.dir === "asc" ? "desc" : "asc" }))}>
              <span class="material-symbols-outlined" style="font-size: 18px;">{$sortPref.dir === "asc" ? "arrow_upward" : "arrow_downward"}</span>
              <span>{$sortPref.dir === "asc" ? "Ascending" : "Descending"}</span>
            </button>
          {/if}
        </div>
      {/if}
    </div>
    <button class="select-btn" class:active={selecting} onclick={toggleSelect} aria-label={selecting ? "Cancel selection" : "Select notes"}>
      <span class="material-symbols-outlined">{selecting ? "check_box" : "checklist_rtl"}</span>
    </button>
    {/if}
  </div>
  </div>

  {#snippet noteCard(note: NoteMetadata)}
      <li>
        {#if selecting}
          <button
            class="note-card"
            class:checked={selected.has(note.id)}
            onclick={() => toggleNote(note.id)}
          >
            <span class="select-box" class:on={selected.has(note.id)}>
              {#if selected.has(note.id)}
                <span class="material-symbols-outlined" style="font-size: 20px; font-variation-settings: 'wght' 700;">check</span>
              {/if}
            </span>
            <div class="note-info">
              <strong>{note.title || "Untitled"}</strong>
              <div class="tags">
                {#each note.tags.slice(0, 3) as tag}<span class="tag">#{tag}</span>{/each}
              </div>
            </div>
            <span class="date">{formatRelative(note.updated_at)}</span>
          </button>
        {:else}
          <div class="note-card"
            class:reordering
            data-row-id={note.id}
            role="button" tabindex="0"
            class:active={desktop && note.id === activeId}
            class:cut={isCut(note.id)}
            class:dark-ink={cardInk(note) === "dark"}
            class:light-ink={cardInk(note) === "light"}
            class:has-bg-image={bgOf(note.id)}
            style:background-color={note.bg_color ?? undefined}
            style:background-image={safeBgImageUrl(bgOf(note.id))}
            onclick={() => { if (!didDrag) goto(`/note/${note.id}`); }}
            onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); goto(`/note/${note.id}`); } }}
            oncontextmenu={(e) => openMenu(e, noteActions(note))}
          >
            {#if reordering}
              <span
                class="drag-grip"
                role="button"
                tabindex="0"
                aria-label={`Reorder ${note.title || "Untitled"}`}
                title="Drag, or press the up and down arrow keys, to move"
                onclick={(e) => e.stopPropagation()}
                onkeydown={(e) => reorderByKey(e, "note", note.id, note.title || "Untitled", (note.pinned ? orderedPinned : orderedNotes).map(x => x.id))}
                onpointerdown={(e) => startReorder(e, "note", note.id, (note.pinned ? orderedPinned : orderedNotes).map(x => x.id))}
              >⠿</span>
            {/if}
            <span class="badge-wrap">
              <span class="kind-badge {noteColor(note)}">
                <span class="material-symbols-outlined">{noteIcon(note)}</span>
              </span>
              {#if note.pinned}
                <span class="pin-indicator"><span class="material-symbols-outlined" style="font-size: 12px; font-variation-settings: 'FILL' 1;">push_pin</span></span>
              {/if}
            </span>
            <div class="note-info">
              <div class="title-row">
                <strong>{note.title || "Untitled"}</strong>
                {#if note.has_note_password}<span class="lock"><span class="material-symbols-outlined" style="font-size: 14px;">lock</span></span>{/if}
              </div>
              <!-- Both slots always render, empty when there is nothing to
                   show. Conditional slots made card height follow content, so a
                   bare note sat noticeably shorter than one with a preview and
                   tags. Reserving them keeps every card the same height without
                   hard-coding one. -->
              <p class="preview-text">
                {#if note.show_preview}
                  {#if note.has_note_password}Locked note{:else if note.preview_text}{note.preview_text}{/if}
                {/if}
              </p>
              <div class="tags">
                {#each note.tags.slice(0, 3) as tag}<span class="tag">#{tag}</span>{/each}
              </div>
            </div>
            <div class="trailing">
              <span class="date">{formatRelative(note.updated_at)}</span>
              <button class="card-menu" aria-label="More options" onclick={(e) => openMenu(e, noteActions(note))}>
                <span class="material-symbols-outlined">more_vert</span>
              </button>
            </div>
          </div>
        {/if}
      </li>
  {/snippet}

  {#if $listTrash}
    <ul class="note-list" aria-label="Trash" bind:this={trashListEl}>
      {#each $trashed as note (note.id)}
        <li>
          <div class="note-card trash-card">
            <span class="badge-wrap">
              <span class="kind-badge {noteColor(note)}">
                <span class="material-symbols-outlined" aria-hidden="true">{noteIcon(note)}</span>
              </span>
            </span>
            <div class="note-info">
              <div class="title-row">
                <strong>{note.title || "Untitled"}</strong>
                {#if note.has_note_password}<span class="lock" role="img" aria-label="Password protected"><span class="material-symbols-outlined" style="font-size: 14px;" aria-hidden="true">lock</span></span>{/if}
              </div>
              <p class="preview-text">
                Deleted <time datetime={new Date(note.deleted_at * 1000).toISOString()}>{deletedFormat.format(new Date(note.deleted_at * 1000))}</time>
              </p>
              <div class="tags"></div>
            </div>
            <div class="trailing">
              <button class="btn-ghost" data-restore onclick={() => restore(note.id)} aria-label={`Restore ${note.title || "Untitled"}`}>
                <span class="material-symbols-outlined" aria-hidden="true">history</span>
              </button>
              <button class="btn-ghost danger" onclick={() => purgeTarget = { ids: [note.id], title: note.title || "Untitled" }} aria-label={`Delete ${note.title || "Untitled"} forever`}>
                <span class="material-symbols-outlined" aria-hidden="true">delete</span>
              </button>
            </div>
          </div>
        </li>
      {/each}
      {#if $trashed.length === 0}
        <li class="empty">
          <span class="material-symbols-outlined empty-icon" aria-hidden="true">delete</span>
          <span>Trash is empty.</span>
        </li>
      {:else}
        <li class="locked-hint">
          <span class="material-symbols-outlined" style="font-size: 14px;" aria-hidden="true">info</span>
          <span>Notes in Trash are deleted forever after 30 days.</span>
        </li>
      {/if}
    </ul>
  {:else}
  <!-- Breadcrumb: only meaningful once you are inside something. -->
  {#if trail().length}
    <nav class="crumbs" aria-label="Folder path">
      <button class="crumb" onclick={() => openFolder(null, desktop && !!activeId)}>
        <!-- aria-hidden: the ligature text is what a screen reader would read,
             and "inbox Home" is not what this button is called. -->
        <span class="material-symbols-outlined" style="font-size: 16px;" aria-hidden="true">inbox</span>
        Home
      </button>
      {#each trail() as c, i (c.id)}
        <span class="crumb-sep" aria-hidden="true">/</span>
        {#if i === trail().length - 1}
          <span class="crumb current" aria-current="page">{c.name}</span>
        {:else}
          <button class="crumb" onclick={() => openFolder(c.id, desktop && !!activeId)}>{c.name}</button>
        {/if}
      {/each}
    </nav>
  {/if}

  <div class="sr-only" role="status" aria-live="polite">{reorderAnnouncement}</div>
  <ul class="note-list">
    <!-- Folders first, as rows you open — the list is a level, not a filter. -->
    {#if !query}
      {#each orderedFolders as f (f.id)}
        <li>
          <div
            class="note-card folder-card"
            class:reordering
            class:cut={isCut(f.id)}
            data-row-id={f.id}
            role="button" tabindex="0"
            onclick={() => { if (!didDrag) openFolder(f.id, desktop && !!activeId); }}
            onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); openFolder(f.id, desktop && !!activeId); } }}
            oncontextmenu={(e) => openMenu(e, folderActions(f))}
          >
            {#if reordering}
              <!-- A span, not a button: on a real touch a button consumes the
                   gesture and the drag never starts. This mirrors the kanban
                   board's handle, which works. role+tabindex keep it reachable. -->
              <span
                class="drag-grip"
                role="button"
                tabindex="0"
                aria-label={`Reorder ${f.name}`}
                title="Drag, or press the up and down arrow keys, to move"
                onclick={(e) => e.stopPropagation()}
                onkeydown={(e) => reorderByKey(e, "folder", f.id, f.name, orderedFolders.map(x => x.id))}
                onpointerdown={(e) => startReorder(e, "folder", f.id, orderedFolders.map(x => x.id))}
              >⠿</span>
            {/if}
            <span class="badge-wrap">
              <span class="kind-badge folder-badge">
                <span class="material-symbols-outlined">folder</span>
              </span>
            </span>
            <div class="note-info">
              <div class="title-row"><strong>{f.name}</strong></div>
              <p class="preview-text">
                {directCount(f.id)} {directCount(f.id) === 1 ? "item" : "items"}
              </p>
              <div class="tags"></div>
            </div>
            <div class="trailing">
              <span class="material-symbols-outlined chevron" style="font-size: 20px;" aria-hidden="true">chevron_right</span>
              <button
                class="card-menu"
                aria-label={`Actions for ${f.name}`}
                onclick={(e) => openMenu(e, folderActions(f))}
              >
                <span class="material-symbols-outlined" aria-hidden="true">more_vert</span>
              </button>
            </div>
          </div>
        </li>
      {/each}
    {/if}
    {#if pinnedNotes.length}
      <li class="section-label">
        <span class="material-symbols-outlined sec-ico" style="font-size: 15px; font-variation-settings: 'FILL' 1;">push_pin</span>
        <span>Pinned</span>
      </li>
      {#each orderedPinned as note (note.id)}{@render noteCard(note)}{/each}
    {/if}
    {#if pinnedNotes.length && otherNotes.length}
      <li class="section-label"><span>All notes</span></li>
    {/if}
    {#each orderedNotes as note (note.id)}{@render noteCard(note)}{/each}
    <!-- An empty folder is not an empty library; saying "no notes yet" when the
         note is one level up reads as data loss. -->
    {#if filtered.length === 0 && (query || childFolders.length === 0)}
      <li class="empty">
        <span class="material-symbols-outlined empty-icon">{filter ? "search_off" : $listFolder ? "folder" : "note_add"}</span>
        <span>
          {#if filter}
            No notes match your search.
          {:else if $listFolder}
            This folder is empty.
          {:else}
            No notes yet. {desktop ? "Use" : "Tap"} + to create one.
          {/if}
        </span>
      </li>
    {/if}
    {#if hiddenLocked}
      <li class="locked-hint">
        <span class="material-symbols-outlined" style="font-size: 14px;">lock</span>
        <span>{hiddenLocked} locked {hiddenLocked === 1 ? "note" : "notes"} hidden</span>
      </li>
    {/if}
    {#if notShown > 0}
      <li class="locked-hint">
        <span class="material-symbols-outlined" style="font-size: 14px;">more_horiz</span>
        <span>Showing {$notes.length} of {$totalNotes} notes</span>
      </li>
    {/if}
  </ul>
  {/if}
</div>

<!-- FAB - touch layout only; desktop composes from the pane header. A bottom
     bar takes its corner while one is up. -->
{#if !desktop && !$listTrash && !selectionBar && !clipboardBar}
  {#if fabOpen}
    <div class="fab-backdrop" role="presentation" onclick={() => fabOpen = false}></div>
  {/if}
  <div class="fab-wrap">
    {#if fabOpen}
      <div class="fab-options">
        {#each fabKinds as kind, i}
          <button class="fab-option" style="animation-delay: {(fabKinds.length - 1 - i) * 40}ms"
            onclick={() => {
              fabOpen = false;
              if (kind.id === "folder") { nameError = ""; nameModal = { mode: "create", initial: "" }; }
              else goto(newNoteHref(kind.id));
            }}>
            <span class="fab-label">{kind.label}</span>
            <span class="fab-badge"><span class="material-symbols-outlined">{kind.icon}</span></span>
          </button>
        {/each}
      </div>
    {/if}
    <button class="fab" class:open={fabOpen} onclick={() => fabOpen = !fabOpen} aria-label="Create note">
      <span class="material-symbols-outlined">add</span>
    </button>
  </div>
{/if}

{#if menu}
  <div class="card-menu-backdrop" role="presentation"
    onclick={() => menu = null}
    oncontextmenu={(e) => { e.preventDefault(); menu = null; }}></div>
  <div class="card-popover" use:anchorMenu={menu.anchor}>
    {#each menu.actions as action}
      <button class="popover-item" class:danger={action.danger} onclick={() => { menu = null; action.run(); }}>
        <span class="material-symbols-outlined" style="font-size: 18px;" aria-hidden="true">{action.icon}</span>
        {action.label}
      </button>
    {/each}
  </div>
{/if}

{#if selectionBar}
  <div class="action-bar" class:desktop>
    <button class="bar-cancel" onclick={toggleSelect} aria-label="Cancel selection">
      <span class="material-symbols-outlined">close</span>
    </button>
    <span class="sel-count">{selected.size} selected</span>
    <div class="bar-spacer"></div>
    <button class="btn-ghost" onclick={() => pwModal = { mode: "set", ids: Array.from(selected), isBatch: true }} aria-label="Protect selected">
      <span class="material-symbols-outlined">lock</span>
    </button>
    <button class="btn-ghost" onclick={() => pwModal = { mode: "remove", ids: Array.from(selected), isBatch: true }} aria-label="Remove protection">
      <span class="material-symbols-outlined">lock_open</span>
    </button>
    <button class="btn-send" onclick={sendSelected}>Send</button>
  </div>
{:else if clipboardBar && $listClipboard}
  {@const n = $listClipboard.ids.length}
  <div class="action-bar" class:desktop>
    <button class="bar-cancel" onclick={() => listClipboard.set(null)} aria-label="Clear clipboard">
      <span class="material-symbols-outlined">close</span>
    </button>
    <span class="sel-count">
      {n} {$listClipboard.kind}{n === 1 ? "" : "s"} {$listClipboard.mode === "cut" ? "cut" : "copied"}
    </span>
    <div class="bar-spacer"></div>
    <button class="btn-send" onclick={() => paste($listFolder)}>Paste here</button>
  </div>
{/if}

{#if showNewNote}
  <NewNoteModal
    onclose={() => showNewNote = false}
    onnewfolder={() => { nameError = ""; nameModal = { mode: "create", initial: "" }; }}
  />
{/if}

{#if nameModal}
  <FolderNameModal
    title={nameModal.mode === "create" ? "New folder" : "Rename folder"}
    confirmLabel={nameModal.mode === "create" ? "Create" : "Rename"}
    initial={nameModal.initial}
    error={nameError}
    onsubmit={submitName}
    onclose={() => { nameModal = null; nameError = ""; }}
  />
{/if}

{#if moveTarget}
  <FolderPickerModal
    title={moveTarget.kind === "note" ? "Move note to" : "Move folder to"}
    current={moveTarget.from}
    excludeSubtreeOf={moveTarget.kind === "folder" ? moveTarget.id : null}
    onpick={submitMove}
    onclose={() => { moveTarget = null; moveError = ""; }}
  />
{/if}

{#if moveError}
  <div class="folder-error" role="alert">
    <span class="material-symbols-outlined" style="font-size: 16px;" aria-hidden="true">warning</span>
    <span>{moveError}</span>
    <button onclick={() => moveError = ""} aria-label="Dismiss">
      <span class="material-symbols-outlined" style="font-size: 16px;" aria-hidden="true">close</span>
    </button>
  </div>
{/if}

{#if pwModal}
  <PasswordModal
    mode={pwModal.mode}
    title={pwHeading}
    onsubmit={handlePassword}
    onclose={() => pwModal = null}
  />
{/if}

{#if transferNoteIds}
  {#await import("$lib/components/TransferModal.svelte") then { default: TransferModal }}
    <TransferModal
      noteIds={transferNoteIds}
      onclose={() => { transferNoteIds = null; selecting = false; selected = new Set(); }}
    />
  {/await}
{/if}

{#if deleteTargets}
  <ConfirmModal
    title="Move to Trash?"
    message={deleteTargets.length === 1
      ? "This note will be moved to Trash. You can restore it from there for 30 days."
      : `${deleteTargets.length} notes will be moved to Trash. You can restore them from there for 30 days.`}
    confirmLabel="Move to Trash"
    destructive
    onconfirm={confirmDelete}
    oncancel={() => deleteTargets = null}
  />
{/if}

{#if purgeTarget}
  <ConfirmModal
    title={purgeTarget === "all" ? "Empty trash?" : "Delete forever?"}
    message={purgeTarget === "all"
      ? `All ${$trashed.length} ${$trashed.length === 1 ? "note" : "notes"} in Trash will be permanently deleted. This cannot be undone.`
      : `"${purgeTarget.title}" will be permanently deleted. This cannot be undone.`}
    confirmLabel={purgeTarget === "all" ? "Empty trash" : "Delete forever"}
    destructive
    onconfirm={confirmPurge}
    oncancel={() => purgeTarget = null}
  />
{/if}

<style>
  .page {
    padding: 1.5rem 2rem 2rem;
    max-width: 600px; margin: 0 auto;
  }

  /* Pinned while the list scrolls under it. It takes over the page's top and
     side padding so the band spans the column, and paints what is behind the
     list - the window gradient, fixed so it lines up with the body's. */
  .list-head {
    position: sticky; top: 0; z-index: 30;
    margin: -1.5rem -2rem 0.5rem; padding: 1.5rem 2rem 0.75rem;
    background: var(--bg-gradient) fixed, var(--bg);
  }

  /* Toolbar */
  .toolbar {
    display: flex; gap: 6px; align-items: center;
    background: var(--surface-glass); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
    padding: 0.4rem 0.5rem; border-radius: var(--radius);
    border: 1px solid var(--border);
    box-shadow: 0 2px 12px var(--shadow-color);
    position: relative; z-index: 10;
  }
  .menu-btn, .sort-btn, .select-btn {
    width: 40px; height: 40px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    border-radius: var(--radius-full); transition: all 0.15s ease;
  }
  .menu-btn:hover, .sort-btn:hover, .select-btn:hover { color: var(--accent); background: var(--accent-muted); }
  .toolbar button > .material-symbols-outlined { font-size: 22px; }
  .search-wrap {
    flex: 1; min-width: 0; display: flex; align-items: center; gap: 6px; height: 38px;
    background: var(--surface-container); border-radius: var(--radius-full);
    padding: 0 0.7rem; transition: box-shadow 0.15s ease;
  }
  .search-wrap:focus-within { box-shadow: 0 0 0 2px var(--accent-muted); }
  .search-icon { color: var(--muted); font-size: 18px; flex-shrink: 0; }
  .search {
    flex: 1; min-width: 0; height: 100%; padding: 0;
    border: none; outline: none; background: transparent; color: var(--text);
    font-size: 0.9rem;
  }
  .search::placeholder { color: var(--muted); }
  .clear-btn {
    display: flex; align-items: center; flex-shrink: 0;
    background: none; border: none; cursor: pointer; color: var(--muted);
    padding: 2px; border-radius: var(--radius-full); transition: all 0.15s ease;
  }
  .clear-btn:hover { color: var(--accent); background: var(--accent-muted); }

  /* Sort */
  .sort-wrap { position: relative; flex-shrink: 0; z-index: 20; }
  .sort-btn.active { color: var(--accent); background: var(--accent-muted); }
  .sort-backdrop { position: fixed; inset: 0; z-index: 19; }
  .sort-dropdown {
    position: absolute; top: calc(100% + 0.5rem); right: 0; z-index: 20;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.3rem; min-width: 170px;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
  }
  .sort-option {
    width: 100%; display: flex; align-items: center; justify-content: flex-start; gap: 10px;
    padding: 0.5rem 0.6rem; border: none; background: none;
    border-radius: var(--radius-sm); cursor: pointer; text-align: left;
    font-size: 0.85rem; color: var(--text-secondary);
    transition: all 0.1s ease;
  }
  .sort-option:hover { background: var(--hover); color: var(--text); }
  .sort-option.active { color: var(--accent); font-weight: 700; }
  .sort-divider { height: 1px; background: var(--border); margin: 4px 6px; }

  .select-btn.active { color: var(--accent); background: var(--accent-muted); }
  @media (max-width: 640px) {
    .page:not(.desktop) { padding: 1rem 0.75rem calc(1rem + env(safe-area-inset-bottom, 0px)); }
    .page:not(.desktop) .list-head { margin: -1rem -0.75rem 0.5rem; padding: 1rem 0.75rem 0.75rem; }
    .search { font-size: 0.85rem; }
  }

  /* Note list */
  /* Breadcrumb — where you are, and a way back up. */
  .crumbs {
    display: flex; align-items: center; flex-wrap: wrap; gap: 2px;
    margin-bottom: 0.6rem; padding: 0 0.15rem;
    font-size: 0.8rem; color: var(--muted); min-height: 32px;
  }
  .crumb {
    display: inline-flex; align-items: center; gap: 4px;
    background: none; border: none; cursor: pointer; color: var(--accent);
    font-size: 0.8rem; font-weight: 600; padding: 4px 6px;
    border-radius: var(--radius-full);
  }
  .crumb:hover { background: var(--hover); }
  .crumb.current { color: var(--text); cursor: default; font-weight: 700; }
  .crumb-sep { opacity: 0.5; }

  /* A folder reads as a card like any other row, but with its own badge tint so
     it is obvious at a glance that opening it goes somewhere. */
  .folder-card .folder-badge { background: var(--secondary-surface); color: var(--secondary); }
  /* A folder's chevron and menu sit mid-row, where its arrow reads as "opens". */
  .folder-card .trailing { color: var(--muted); gap: 0; }

  /* Surfaced backend refusals — a cycle, or too deep. */
  .folder-error {
    position: fixed; z-index: 302;
    left: 50%; transform: translateX(-50%);
    bottom: calc(1.5rem + env(safe-area-inset-bottom, 0px));
    display: flex; align-items: center; gap: 0.5rem;
    max-width: min(420px, calc(100vw - 2rem));
    padding: 0.6rem 0.5rem 0.6rem 0.85rem;
    border-radius: var(--radius-full);
    background: var(--surface); border: 1px solid var(--error);
    color: var(--text); font-size: 0.85rem;
    box-shadow: 0 8px 24px var(--shadow-color-hover);
  }
  .folder-error button {
    border: none; background: none; cursor: pointer; color: var(--muted);
    display: flex; align-items: center; justify-content: center;
    width: 32px; height: 32px; border-radius: var(--radius-full);
  }
  .folder-error button:hover { color: var(--text); background: var(--hover); }

  /* Trash: the same cards, but there is nothing to open, so no pointer or lift. */
  .trash-title {
    flex: 1; min-width: 0; margin: 0; padding-left: 0.5rem;
    font-size: 1rem; font-weight: 800; color: var(--text);
  }
  .trash-title:focus { outline: none; }
  .empty-trash {
    height: 38px; padding: 0 1rem; flex-shrink: 0;
    border: 1px solid var(--border); border-radius: var(--radius-full);
    background: none; color: var(--error); cursor: pointer;
    font-size: 0.85rem; font-weight: 600;
    transition: all 0.15s ease;
  }
  .empty-trash:hover:not(:disabled) { border-color: var(--error); background: var(--error-surface); }
  .empty-trash:disabled { opacity: 0.45; cursor: default; }
  .note-card.trash-card { cursor: default; }
  .note-card.trash-card:hover { transform: none; }
  .trash-card .trailing { align-self: center; gap: 0.35rem; }
  .btn-ghost.danger:hover { border-color: var(--error); color: var(--error); background: var(--error-surface); }
  .page.desktop .trash-card .btn-ghost { width: 34px; height: 34px; }

  /* Grip only appears in arrange mode, so the row is not permanently cluttered
     and a scroll gesture cannot catch it by accident. */
  .drag-grip {
    border: none; background: none;
    display: flex; align-items: center; justify-content: center;
    width: 28px; min-height: 44px; flex-shrink: 0;
    color: var(--muted); cursor: grab; touch-action: none;
    font-size: 1.1rem; user-select: none;
  }
  .drag-grip:active { cursor: grabbing; }
  .sr-only {
    position: absolute; width: 1px; height: 1px;
    padding: 0; margin: -1px; overflow: hidden;
    clip-path: inset(50%); white-space: nowrap; border: 0;
  }
  .note-card.reordering { cursor: default; }
  .note-card.reordering:hover { transform: none; }

  .note-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 0.6rem; }
  li { display: flex; align-items: center; min-width: 0; position: relative; }
  .note-card {
    flex: 1; min-width: 0; display: flex; align-items: flex-start; gap: 0.8rem;
    padding: 0.85rem 0.95rem; border-radius: var(--radius);
    /* background-color, never the `background` shorthand: these cards can carry
       a user image, and the shorthand resets background-position/size, which
       knocks a centred cover image back to top-left at its natural size. */
    border: 1px solid transparent; background-color: var(--surface);
    text-decoration: none; color: var(--text);
    cursor: pointer; width: 100%; text-align: left;
    box-shadow: 0 4px 16px var(--shadow-color);
    transition: all 0.2s ease;
  }
  .note-card:hover {
    background-color: var(--hover);
    box-shadow: 0 8px 24px var(--shadow-color-hover);
    transform: translateY(-2px);
    border-color: var(--accent-muted);
  }
  .note-card:focus-visible {
    outline: none;
    box-shadow: 0 0 0 2px var(--accent), 0 4px 16px var(--shadow-color);
  }
  .note-card.checked { border-color: var(--accent); background-color: var(--accent-muted); }
  /* Waiting to be pasted somewhere else. */
  .note-card.cut { opacity: 0.5; }

  /* The menu is always the last thing on a row, at its right edge. */
  .trailing {
    display: flex; align-items: center; align-self: flex-start;
    flex-shrink: 0; gap: 0.15rem;
  }
  .title-row { display: flex; align-items: center; gap: 6px; min-width: 0; }

  .badge-wrap { position: relative; flex-shrink: 0; }
  .pin-indicator {
    position: absolute; top: -4px; right: -4px;
    background: var(--accent); color: var(--on-accent);
    width: 18px; height: 18px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    box-shadow: 0 1px 4px var(--shadow-color);
    pointer-events: none;
  }
  .select-box {
    width: 40px; height: 40px; border-radius: 12px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    border: 2px solid var(--border); background: transparent; color: var(--on-accent);
    transition: all 0.15s ease;
  }
  .select-box.on { border-color: var(--accent); background: var(--accent); }

  .kind-badge {
    width: 40px; height: 40px; border-radius: 12px;
    display: flex; align-items: center; justify-content: center;
    flex-shrink: 0; font-size: 1rem;
  }
  .kind-badge.accent { background: var(--accent-surface); color: var(--accent); }
  .kind-badge.secondary { background: var(--secondary-surface); color: var(--secondary); }
  .kind-badge.tertiary { background: var(--tertiary-surface); color: var(--tertiary); }

  .note-info { flex: 1; min-width: 0; overflow: hidden; }
  .note-info strong {
    min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
    font-weight: 700; font-size: 0.95rem;
  }
  .lock { color: var(--muted); display: flex; flex-shrink: 0; }
  /* Always occupies two clamped lines, so a one-line preview, a two-line one and
     an empty one are the same height. Height comes from the type, not a magic
     number, so it survives a font-size change. */
  .preview-text {
    margin: 3px 0 0; font-size: 0.78rem; color: var(--muted); line-height: 1.4;
    display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical;
    overflow: hidden;
    min-height: calc(2 * 1.4 * 0.78rem);
  }
  /* One row, never two. Tags wrap inside a box one tag tall, so one that does
     not fit drops out of sight whole instead of being sliced at the edge. */
  .tags {
    display: flex; gap: 6px; margin-top: 8px; flex-wrap: wrap;
    overflow: hidden; height: calc(0.68rem * 1.3 + 4px);
  }
  .tag {
    font-size: 0.68rem; line-height: 1.3; padding: 2px 10px; font-weight: 600; white-space: nowrap;
    background: var(--accent-muted); border-radius: var(--radius-full); color: var(--accent);
  }
  .date { font-size: 0.72rem; color: var(--muted); white-space: nowrap; font-weight: 500; }

  /* Auto-contrast ink for notes with a custom background */
  .note-card.dark-ink { color: #2e1a28; }
  .note-card.dark-ink .date,
  .note-card.dark-ink .preview-text,
  .note-card.dark-ink .lock,
  .note-card.dark-ink .card-menu { color: #604868; }
  .note-card.dark-ink .tag { background: rgba(0,0,0,0.08); color: #604868; }

  .note-card.light-ink { color: #ffffff; }
  .note-card.light-ink .date,
  .note-card.light-ink .preview-text,
  .note-card.light-ink .lock,
  .note-card.light-ink .card-menu { color: rgba(255,255,255,0.88); }
  .note-card.light-ink .tag { background: rgba(255,255,255,0.22); color: #ffffff; }

  .note-card.has-bg-image {
    background-size: cover; background-position: center center;
    background-repeat: no-repeat;
    position: relative;
  }
  .note-card > * { position: relative; z-index: 1; }
  .note-card.has-bg-image::before {
    content: ""; position: absolute; inset: 0; z-index: 0;
    border-radius: inherit; pointer-events: none;
  }
  .note-card.has-bg-image.dark-ink::before { background: rgba(255,255,255,0.45); }
  .note-card.has-bg-image.light-ink::before { background: rgba(0,0,0,0.4); }

  .section-label {
    display: flex; align-items: center; gap: 6px;
    padding: 0.6rem 0.5rem 0.1rem;
    font-size: 0.72rem; font-weight: 700; letter-spacing: 0.04em;
    text-transform: uppercase; color: var(--muted);
  }
  .section-label .sec-ico { color: var(--accent); }

  /* Card context menu */
  .card-menu {
    background: none; border: none; cursor: pointer; margin: 0 -2px -2px 0;
    color: var(--muted); flex-shrink: 0; padding: 0.2rem;
    border-radius: var(--radius-full); transition: all 0.15s ease;
    display: flex; align-items: center;
  }
  .card-menu:hover { color: var(--text); background: var(--hover); }
  /* Above the sticky header and the FAB, which would otherwise cover it. */
  .card-menu-backdrop { position: fixed; inset: 0; z-index: 89; }
  /* Fixed and placed by anchorMenu, so no row or scroller can clip it. */
  .card-popover {
    position: fixed; z-index: 90;
    max-height: calc(100vh - 16px); overflow-y: auto;
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    border: 1px solid var(--border); border-radius: var(--radius);
    padding: 0.25rem; box-shadow: 0 8px 24px var(--shadow-color-hover);
    min-width: 140px;
  }
  /* On touch the menu is a sheet at the bottom instead: nothing to clip it, and
     the rows get a real tap target. */
  @media (hover: none) {
    .card-menu-backdrop { z-index: 309; background: var(--backdrop); }
    .card-popover {
      position: fixed; left: 0; right: 0; bottom: 0; top: auto;
      z-index: 310; min-width: 0;
      border-radius: var(--radius-lg) var(--radius-lg) 0 0;
      padding: 0.5rem 0.5rem calc(0.5rem + max(env(safe-area-inset-bottom, 0px), 24px));
      max-height: 70vh; overflow-y: auto;
    }
    .popover-item { min-height: 48px; font-size: 0.95rem; padding: 0.75rem 1rem; }
  }

  .popover-item {
    width: 100%; display: flex; align-items: center; gap: 0.5rem;
    padding: 0.5rem 0.75rem; border: none; background: none;
    border-radius: var(--radius-sm); cursor: pointer; font-size: 0.85rem;
    color: var(--text);
    transition: background 0.1s ease;
  }
  .popover-item:hover { background: var(--hover); }
  .popover-item.danger { color: var(--error); }

  /* Quiet footnote, not a warning: the exclusion is intentional. */
  .locked-hint {
    justify-content: center; gap: 6px;
    padding: 0.7rem 0.5rem 0.2rem;
    color: var(--muted); font-size: 0.75rem; font-weight: 500;
  }

  .empty {
    color: var(--muted); padding: 3rem; text-align: center; font-weight: 500;
    display: flex; flex-direction: column; align-items: center; gap: 0.75rem;
  }
  .empty-icon { font-size: 48px; opacity: 0.4; }

  /* Action bar */
  .action-bar {
    position: fixed; left: 0; right: 0; z-index: 60;
    bottom: env(safe-area-inset-bottom, 0px);
    background: var(--surface-glass); backdrop-filter: blur(12px); -webkit-backdrop-filter: blur(12px);
    border-top: 1px solid var(--border);
    padding: 0.8rem 1rem;
    display: flex; align-items: center; gap: 0.6rem;
    box-shadow: 0 -4px 16px var(--shadow-color);
  }
  .bar-cancel {
    width: 40px; height: 40px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    background: none; border: none; cursor: pointer; color: var(--text-secondary);
    border-radius: var(--radius-full); transition: all 0.15s ease;
  }
  .bar-cancel:hover { color: var(--accent); background: var(--accent-muted); }
  .sel-count { font-size: 0.9rem; color: var(--text); font-weight: 700; }
  .bar-spacer { flex: 1; }
  .btn-ghost {
    width: 40px; height: 40px; border-radius: var(--radius-full);
    border: 1px solid var(--border); background: transparent;
    color: var(--text-secondary); cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    transition: all 0.15s ease;
  }
  .btn-ghost:hover { border-color: var(--accent); color: var(--accent); background: var(--accent-muted); }
  .btn-ghost .material-symbols-outlined { font-size: 20px; }
  .btn-send {
    padding: 0.6rem 1.4rem; border-radius: var(--radius-full);
    border: none; background: var(--accent); color: var(--on-accent);
    font-weight: 600; cursor: pointer;
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease;
  }
  .btn-send:hover { transform: scale(1.03); }

  /* FAB */
  .fab-backdrop { position: fixed; inset: 0; z-index: 70; background: rgba(0,0,0,0.25); }
  .fab-wrap {
    position: fixed; right: 1.5rem; bottom: calc(1.5rem + env(safe-area-inset-bottom, 0px));
    z-index: 80; display: flex; flex-direction: column; align-items: flex-end; gap: 0.75rem;
  }
  .fab {
    width: 56px; height: 56px; border-radius: var(--radius-full); border: none;
    background: var(--accent); color: var(--on-accent); cursor: pointer;
    display: flex; align-items: center; justify-content: center;
    box-shadow: 0 6px 20px var(--shadow-color-hover);
    transition: transform 0.2s ease, background 0.15s ease;
  }
  .fab .material-symbols-outlined { font-size: 28px; transition: transform 0.25s ease; }
  .fab.open .material-symbols-outlined { transform: rotate(45deg); }
  .fab:hover { transform: scale(1.08); }
  .fab-options { display: flex; flex-direction: column; align-items: flex-end; gap: 0.5rem; }
  .fab-option {
    display: flex; align-items: center; gap: 0.6rem; padding: 0.5rem 0.55rem 0.5rem 1rem;
    border: 1px solid var(--border); border-radius: var(--radius-full);
    background: var(--surface-glass); backdrop-filter: blur(16px); -webkit-backdrop-filter: blur(16px);
    color: var(--text); cursor: pointer; font-size: 0.85rem; font-weight: 600;
    box-shadow: 0 4px 16px var(--shadow-color);
    animation: fab-pop 0.2s ease both;
    transition: background 0.1s ease, transform 0.1s ease;
    white-space: nowrap;
  }
  .fab-option:hover { background: var(--hover); transform: translateX(-4px); }
  .fab-label { line-height: 1; }
  .fab-badge {
    width: 34px; height: 34px; border-radius: var(--radius-full);
    background: var(--accent-muted); color: var(--accent);
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
  }
  .fab-badge .material-symbols-outlined { font-size: 18px; }
  @keyframes fab-pop {
    from { opacity: 0; transform: translateY(8px) scale(0.9); }
    to { opacity: 1; transform: translateY(0) scale(1); }
  }

  /* ---- Desktop pane variant ----
     A 320px column reads as a list, not a deck of floating cards: flat rows,
     hairline separators, tinted active row. Touch layout above is untouched. */
  .page.desktop {
    max-width: none; margin: 0;
    padding: 0.75rem 0.6rem 1rem;
  }
  .pane-head {
    display: flex; align-items: center; gap: 0.35rem;
    padding: 0 0.25rem 0.5rem;
  }
  .wordmark {
    flex: 1; min-width: 0;
    font-size: 1.05rem; font-weight: 900; letter-spacing: -0.02em; color: var(--text);
  }
  .compose-btn {
    width: 34px; height: 34px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    border: none; border-radius: var(--radius-full); cursor: pointer;
    background: var(--accent); color: var(--on-accent);
    box-shadow: 0 2px 8px var(--shadow-color);
    transition: transform 0.1s ease, background 0.15s ease;
  }
  .compose-btn:hover { background: var(--accent-hover); transform: scale(1.06); }
  .compose-btn:active { transform: scale(0.95); }
  .compose-btn .material-symbols-outlined { font-size: 22px; }

  /* The pane's own glass over the window gradient, as behind the rows. */
  .page.desktop .list-head {
    margin: -0.75rem -0.6rem 0.25rem; padding: 0.75rem 0.6rem 0.25rem;
    background: linear-gradient(var(--surface-glass), var(--surface-glass)), var(--bg-gradient) fixed, var(--bg);
  }
  .page.desktop .toolbar {
    background: none; backdrop-filter: none; -webkit-backdrop-filter: none;
    border: none; box-shadow: none;
    padding: 0 0.25rem;
  }
  /* Desktop stays a little denser than touch, but it is still a list of cards:
     `gap: 0` with hairline separators made the same list look unlike the phone.
     Cards get real spacing, so the separators are gone with it. */
  .page.desktop .note-list { gap: 0.4rem; }
  .page.desktop .note-card {
    border-radius: var(--radius);
    box-shadow: 0 2px 8px var(--shadow-color);
    padding: 0.7rem;
    gap: 0.7rem;
  }
  /* No lift on hover: rows nudging themselves upward reads as jitter in a
     dense list you are scanning with a mouse. */
  .page.desktop .note-card:hover {
    transform: none;
    box-shadow: 0 4px 14px var(--shadow-color-hover);
    background-color: var(--hover);
  }
  .page.desktop .note-card.active {
    background: var(--accent-muted); border-color: var(--accent-muted);
  }
  .page.desktop .note-card.active strong { color: var(--accent); }
  .page.desktop .kind-badge, .page.desktop .select-box { width: 34px; height: 34px; border-radius: 10px; }
  .page.desktop .kind-badge .material-symbols-outlined { font-size: 19px; }
  .page.desktop .note-info strong { font-size: 0.88rem; }
  .page.desktop .preview-text { font-size: 0.75rem; -webkit-line-clamp: 1; line-clamp: 1; }
  .page.desktop .empty { padding: 2rem 1rem; font-size: 0.85rem; }

  /* Scoped to the list column instead of spanning the whole window. */
  .action-bar.desktop {
    position: sticky; left: auto; right: auto;
    bottom: 0; margin-top: 0.5rem;
    border-radius: var(--radius); border: 1px solid var(--border);
    padding: 0.6rem 0.7rem; gap: 0.4rem;
  }
  .action-bar.desktop .sel-count { font-size: 0.8rem; }
  .action-bar.desktop .btn-ghost, .action-bar.desktop .bar-cancel { width: 34px; height: 34px; }
  .action-bar.desktop .btn-send { padding: 0.45rem 0.9rem; font-size: 0.85rem; }
</style>
