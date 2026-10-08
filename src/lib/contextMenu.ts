/// One row of a note or folder menu: a popover on desktop, a bottom sheet on touch.
export interface MenuAction {
  label: string;
  icon: string;
  danger?: boolean;
  run: () => void;
}

export interface Rect { left: number; top: number; right: number; bottom: number; }

/// Where a menu of `size` goes so it stays inside the viewport: below the
/// anchor unless there is no room, then above; starting at its left edge unless
/// that overflows, then ending at its right edge. Clamped either way.
export function placeMenu(
  anchor: Rect,
  size: { width: number; height: number },
  viewport: { width: number; height: number },
  margin = 8,
): { left: number; top: number } {
  const clamp = (v: number, max: number) => Math.max(margin, Math.min(v, max - margin));
  const fitsBelow = anchor.bottom + size.height <= viewport.height - margin;
  const top = fitsBelow ? anchor.bottom : anchor.top - size.height;
  const fitsRight = anchor.left + size.width <= viewport.width - margin;
  const left = fitsRight ? anchor.left : anchor.right - size.width;
  return {
    left: clamp(left, viewport.width - size.width),
    top: clamp(top, viewport.height - size.height),
  };
}

/// Svelte action: pin a fixed popover next to `anchor`. Touch keeps its CSS
/// bottom sheet, so it is left alone there.
export function anchorMenu(node: HTMLElement, anchor: Rect) {
  const place = (at: Rect) => {
    if (window.matchMedia?.("(hover: none)").matches) return;
    const { width, height } = node.getBoundingClientRect();
    const { left, top } = placeMenu(at, { width, height }, { width: window.innerWidth, height: window.innerHeight });
    node.style.left = `${left}px`;
    node.style.top = `${top}px`;
  };
  place(anchor);
  return { update: place };
}
