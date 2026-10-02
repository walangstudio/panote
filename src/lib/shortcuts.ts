/// Every list keyboard shortcut. Ctrl/Cmd+S belongs to the note editor, which
/// owns the save.
export type Shortcut = "new-note" | "new-folder" | "delete" | "find" | "escape" | "copy" | "cut" | "paste";

function typing(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName));
}

const textSelected = () => window.getSelection()?.isCollapsed === false;

/// Which shortcut `e` is, if any. Keys typed into a field are the field's,
/// except Escape. Copy and cut leave selected text to the browser.
export function shortcutFor(e: KeyboardEvent): Shortcut | null {
  if (e.key === "Escape") return "escape";
  if (typing(e.target)) return null;
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  if (mod && key === "n") return e.shiftKey ? "new-folder" : "new-note";
  if (mod && !e.shiftKey && key === "f") return "find";
  if (mod && !e.shiftKey && (key === "c" || key === "x")) return textSelected() ? null : key === "c" ? "copy" : "cut";
  if (mod && !e.shiftKey && key === "v") return "paste";
  if (!mod && !e.shiftKey && !e.altKey && e.key === "Delete") return "delete";
  return null;
}

/// Ctrl/Cmd+F searches the open note, unless focus is in the list pane, where
/// it searches the list. Both handlers ask this, so exactly one of them acts.
export function findBelongsToNote(target: EventTarget | null): boolean {
  return !!document.querySelector("[data-find-root]")
    && !(target as Element | null)?.closest?.("[data-list-pane]");
}
