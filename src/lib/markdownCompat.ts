/// Detecting markdown a note contains but the editor's schema cannot hold.
///
/// markdown-it parses a SUPERSET of what the ProseMirror schema can represent.
/// Anything in that gap is parsed, dropped to its text, and then written back on
/// the first edit — silently rewriting the note. Tables and images were the worst
/// of it and now have schema nodes; these are what is left.
///
/// This does not fix the round trip. It makes it visible BEFORE the first
/// keystroke, so the user chooses instead of finding out afterwards.

export interface LossyConstruct {
  id: string;
  /// Shown to the user, so phrased as what happens to their note.
  label: string;
}

const FRONT_MATTER = /^---\r?\n[\s\S]*?\r?\n---(\r?\n|$)/;
/// `[label]: https://…` on its own line. markdown-it resolves these into inline
/// links, so the definition block disappears entirely.
const REFERENCE_DEF = /^[ \t]{0,3}\[[^\]]+\]:[ \t]*\S+/m;
/// A block-level HTML tag at the start of a line. Inline spans (colour,
/// highlight) round-trip fine and must not trigger this.
const HTML_BLOCK = /^[ \t]{0,3}<(\/?)(?!span|br|em|strong|code|a|mark|u\b)[a-zA-Z][a-zA-Z0-9-]*(\s|>|\/>)/m;
/// Text underlined with = or - becomes an ATX heading on the way back.
const SETEXT = /^(?!\s*$).+\r?\n[ \t]{0,3}(=+|-{2,})[ \t]*$/m;
const FOOTNOTE = /^[ \t]{0,3}\[\^[^\]]+\]:/m;

/// Four-space indented code, excluding list continuations.
///
/// A list item's continuation lines are indented too — and may sit after a blank
/// line — so the blank cannot be the stopping point. What distinguishes them is
/// the nearest non-blank line ABOVE: a list marker (or another indented line
/// belonging to that list) means continuation, anything else means code.
function hasIndentedCodeBlock(md: string): boolean {
  const lines = md.split(/\r?\n/);
  const isListMarker = (l: string) => /^\s*([-+*]|\d+[.)])\s/.test(l);

  for (let i = 0; i < lines.length; i++) {
    if (!/^ {4}\S/.test(lines[i])) continue;

    let inList = false;
    for (let j = i - 1; j >= 0; j--) {
      if (/^\s*$/.test(lines[j])) continue; // blanks don't end a list
      inList = isListMarker(lines[j]) || /^ {2,}\S/.test(lines[j]);
      break;
    }
    if (!inList) return true;
  }
  return false;
}

/// Everything in `body` that will not survive a parse/serialise round trip.
export function detectLossyConstructs(body: string): LossyConstruct[] {
  if (!body) return [];
  const found: LossyConstruct[] = [];

  if (FRONT_MATTER.test(body)) {
    found.push({
      id: "front-matter",
      label: "front matter (the --- block becomes two horizontal rules)",
    });
  }
  if (REFERENCE_DEF.test(body)) {
    found.push({
      id: "reference-links",
      label: "reference-style links (definitions are removed, links become inline)",
    });
  }
  if (HTML_BLOCK.test(body)) {
    found.push({ id: "html-block", label: "raw HTML blocks (tags are stripped)" });
  }
  if (SETEXT.test(body)) {
    found.push({ id: "setext", label: "underlined headings (rewritten as # headings)" });
  }
  if (hasIndentedCodeBlock(body)) {
    found.push({ id: "indented-code", label: "indented code blocks (rewritten as ``` fences)" });
  }
  if (FOOTNOTE.test(body)) {
    found.push({ id: "footnotes", label: "footnotes (kept as plain text, no longer linked)" });
  }
  return found;
}
