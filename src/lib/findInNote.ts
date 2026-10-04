/// Find in note: matches are painted with the CSS Custom Highlight API, so the
/// note's DOM, and with it its content, is never touched. Text inside <input>
/// and <textarea> is not in the DOM, so those matches are shown by selecting
/// them in the field instead.

export type Match =
  | { kind: "range"; range: Range }
  | { kind: "field"; el: HTMLInputElement | HTMLTextAreaElement; start: number; end: number };

/// Start offsets of each case-insensitive, non-overlapping `query` in `text`.
export function matchOffsets(text: string, query: string): number[] {
  if (!query) return [];
  const re = new RegExp(query.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "giu");
  return [...text.matchAll(re)].map(m => m.index!);
}

export function stepMatch(current: number, count: number, dir: 1 | -1): number {
  return count ? (current + dir + count) % count : 0;
}

export function matchLabel(current: number, count: number, query: string): string {
  if (!query) return "";
  return count ? `${current + 1} of ${count}` : "No matches";
}

const isTextField = (el: Element): el is HTMLInputElement | HTMLTextAreaElement =>
  el instanceof HTMLTextAreaElement
  || (el instanceof HTMLInputElement && /^(text|search|url|email|tel)$/.test(el.type));

const shown = (el: Element) => !el.checkVisibility || el.checkVisibility();

/// Every match under `root`, in document order, leaving out `exclude` (the find
/// bar itself).
export function collectMatches(root: Element, query: string, exclude?: Element | null): Match[] {
  const out: Match[] = [];
  if (!query) return out;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT, {
    acceptNode: (n) =>
      exclude?.contains(n) ? NodeFilter.FILTER_REJECT
      : n.nodeType === Node.ELEMENT_NODE && !shown(n as Element) ? NodeFilter.FILTER_REJECT
      : NodeFilter.FILTER_ACCEPT,
  });
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    if (n.nodeType === Node.TEXT_NODE) {
      if (n.parentElement && isTextField(n.parentElement)) continue;
      for (const at of matchOffsets(n.textContent ?? "", query)) {
        const range = document.createRange();
        range.setStart(n, at);
        range.setEnd(n, at + query.length);
        out.push({ kind: "range", range });
      }
    } else if (isTextField(n as Element)) {
      const el = n as HTMLInputElement | HTMLTextAreaElement;
      for (const at of matchOffsets(el.value, query)) out.push({ kind: "field", el, start: at, end: at + query.length });
    }
  }
  return out;
}

const ALL = "find-match";
const CURRENT = "find-current";

/// Paint `matches`, with `current` set apart, and bring it into view.
export function showMatches(matches: Match[], current: number) {
  if (typeof CSS === "undefined" || !("highlights" in CSS)) return;
  const ranges = matches.flatMap(m => (m.kind === "range" ? [m.range] : []));
  CSS.highlights.set(ALL, new Highlight(...ranges));
  const m = matches[current];
  if (m?.kind === "range") {
    CSS.highlights.set(CURRENT, new Highlight(m.range));
    m.range.startContainer.parentElement?.scrollIntoView({ block: "center" });
  } else {
    CSS.highlights.delete(CURRENT);
  }
}

export function clearMatches() {
  if (typeof CSS === "undefined" || !("highlights" in CSS)) return;
  CSS.highlights.delete(ALL);
  CSS.highlights.delete(CURRENT);
}
