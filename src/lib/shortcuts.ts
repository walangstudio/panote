/// Every list keyboard shortcut. Ctrl/Cmd+S belongs to the note editor, which
/// owns the save.
export type Shortcut = "new-note" | "new-folder" | "delete" | "find" | "escape";

function typing(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  return !!el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName));
}

/// Which shortcut `e` is, if any. Keys typed into a field are the field's,
/// except Escape.
export function shortcutFor(e: KeyboardEvent): Shortcut | null {
  if (e.key === "Escape") return "escape";
  if (typing(e.target)) return null;
  const mod = e.ctrlKey || e.metaKey;
  const key = e.key.toLowerCase();
  if (mod && key === "n") return e.shiftKey ? "new-folder" : "new-note";
  if (mod && !e.shiftKey && key === "f") return "find";
  if (!mod && !e.shiftKey && !e.altKey && e.key === "Delete") return "delete";
  return null;
}
