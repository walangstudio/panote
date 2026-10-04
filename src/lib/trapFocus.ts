import type { Action } from "svelte/action";

// Keeps Tab/Shift+Tab cycling within `node` while it's mounted. Every modal
// already moves focus in on mount and restores it on close; this is the
// missing piece — without it Tab still walks out into the page behind the
// dialog.
const FOCUSABLE =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

export const trapFocus: Action<HTMLElement> = (node) => {
  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Tab") return;
    const els = [...node.querySelectorAll<HTMLElement>(FOCUSABLE)];
    if (els.length === 0) return;
    const first = els[0];
    const last = els[els.length - 1];
    if (e.shiftKey && document.activeElement === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault();
      first.focus();
    }
  }

  node.addEventListener("keydown", onKeydown);
  return {
    destroy() {
      node.removeEventListener("keydown", onKeydown);
    },
  };
};
