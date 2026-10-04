import { Table, TableCell, TableHeader, TableRow } from "@tiptap/extension-table";
import type { Node as PMNode } from "@tiptap/pm/model";

/// Serialising a table cell's inline content to markdown.
///
/// prosemirror-markdown's serializer writes into a shared output buffer and has
/// no "render this node to a string" entry point, so cell text is rendered here
/// instead. Only inline marks are handled — a cell containing block structure
/// (a list, a nested table) is flattened to its text.
///
/// ponytail: covers the marks GFM tables actually carry; extend if a real note
/// turns up needing more.
/// A literal pipe would end the cell; a newline would end the row.
///
/// Backslashes go first, and the order is the whole point: escaping only the
/// pipe turns the cell text `a\|b` into `a\\|b`, which reads back as an escaped
/// backslash followed by a live separator - the row splits and the table is
/// corrupted on the next load. Exported for the test that pins that order.
export function escapeCell(text: string): string {
  return text
    .replace(/\\/g, "\\\\")
    .replace(/\|/g, "\\|")
    .replace(/\r?\n/g, " ")
    .trim();
}

function inlineToMarkdown(node: PMNode): string {
  let out = "";
  node.descendants((child) => {
    if (!child.isText) return true;
    let text = child.text ?? "";
    for (const mark of child.marks) {
      switch (mark.type.name) {
        case "bold": text = `**${text}**`; break;
        case "italic": text = `*${text}*`; break;
        case "strike": text = `~~${text}~~`; break;
        case "code": text = `\`${text}\``; break;
        case "link": text = `[${text}](${mark.attrs.href})`; break;
        default: break;
      }
    }
    out += text;
    return false;
  });
  return escapeCell(out);
}

function cellsOf(row: PMNode): string[] {
  const cells: string[] = [];
  row.forEach((cell) => cells.push(inlineToMarkdown(cell)));
  return cells;
}

/// GFM pipe-table serialiser.
///
/// Without this, tiptap-markdown falls back to `HTMLNode` and writes the whole
/// table out as raw `<table>` markup — content survives, but the note stops
/// being readable markdown. Notes are stored as markdown, so they stay markdown.
export const MarkdownTable = Table.extend({
  addStorage() {
    return {
      ...this.parent?.(),
      markdown: {
        serialize(state: { write: (s: string) => void; closeBlock: (n: PMNode) => void }, node: PMNode) {
          const rows: string[][] = [];
          node.forEach((row) => rows.push(cellsOf(row)));
          if (rows.length === 0) return;

          // GFM requires a header row; a header-less table gets an empty one so
          // it still parses back as a table rather than as paragraphs.
          const width = Math.max(...rows.map((r) => r.length));
          const pad = (r: string[]) => {
            const copy = [...r];
            while (copy.length < width) copy.push("");
            return copy;
          };

          const [header, ...body] = rows;
          state.write(`| ${pad(header).join(" | ")} |\n`);
          state.write(`| ${pad([]).map(() => "---").join(" | ")} |\n`);
          for (const row of body) state.write(`| ${pad(row).join(" | ")} |\n`);
          state.closeBlock(node);
        },
      },
    };
  },
});

export const tableExtensions = [
  MarkdownTable.configure({ resizable: false }),
  TableRow,
  TableHeader,
  TableCell,
];
