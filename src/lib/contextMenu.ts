import { Menu } from "@tauri-apps/api/menu";

/// One row of a note or folder menu. The native menu and the in-app popover
/// both render from the same list, so the two can never offer different things.
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

/// Native menus are a desktop thing: Android has no popup menu, and a touch
/// screen gets the bottom sheet instead.
function nativeMenus(): boolean {
  return !!window.matchMedia?.("(hover: hover) and (pointer: fine)").matches
    && !/Android/i.test(navigator.userAgent);
}

// Each Menu is a backend resource. Closing it right after popup() could race
// the item's click event, so the previous one is released on the next open.
let lastMenu: Menu | null = null;

/// Show `actions` as an OS context menu at the cursor, which the OS keeps on
/// screen. `fallback` runs where that is not possible (touch, or no Tauri
/// runtime as under Playwright).
export function showMenu(actions: MenuAction[], fallback: () => void) {
  if (!nativeMenus()) return fallback();
  (async () => {
    lastMenu?.close().catch(() => {});
    lastMenu = await Menu.new({ items: actions.map(a => ({ text: a.label, action: () => a.run() })) });
    await lastMenu.popup();
  })().catch(fallback);
}
