import { describe, it, expect } from "vitest";
import { detectLossyConstructs } from "./markdownCompat";

const ids = (md: string) => detectLossyConstructs(md).map(c => c.id);

describe("detects constructs the schema cannot hold", () => {
  it("front matter", () => {
    expect(ids("---\ntitle: My Note\ntags: [a]\n---\n\nBody")).toContain("front-matter");
  });

  it("reference-style link definitions", () => {
    expect(ids("See [the docs][d].\n\n[d]: https://example.com")).toContain("reference-links");
  });

  it("raw HTML blocks", () => {
    expect(ids("<div class=\"note\">\nhello\n</div>")).toContain("html-block");
  });

  it("underlined (setext) headings", () => {
    expect(ids("My Title\n========\n\nbody")).toContain("setext");
  });

  it("four-space indented code", () => {
    expect(ids("Some text\n\n    let x = 1;\n    let y = 2;")).toContain("indented-code");
  });

  it("footnote definitions", () => {
    expect(ids("Text[^1]\n\n[^1]: the note")).toContain("footnotes");
  });

  it("reports several at once", () => {
    const md = "---\na: b\n---\n\nTitle\n=====\n\n[d]: https://x.com";
    expect(ids(md)).toEqual(expect.arrayContaining(["front-matter", "setext", "reference-links"]));
  });
});

// A warning that fires on ordinary notes is worse than no warning — people learn
// to dismiss it. These are the cases that must stay silent.
describe("does not fire on markdown that round-trips fine", () => {
  it("plain prose", () => {
    expect(ids("Just some text.\n\nAnother paragraph.")).toEqual([]);
  });

  it("ATX headings, lists, quotes and fenced code", () => {
    const md = "# Title\n\n- one\n- two\n\n> quoted\n\n```js\nlet x = 1;\n```";
    expect(ids(md)).toEqual([]);
  });

  it("a horizontal rule that is not front matter", () => {
    expect(ids("Some text\n\n---\n\nMore text")).not.toContain("front-matter");
  });

  it("a GFM table, which now has schema support", () => {
    expect(ids("| a | b |\n| --- | --- |\n| 1 | 2 |")).toEqual([]);
  });

  it("an image, which now has schema support", () => {
    expect(ids("![alt](data:image/png;base64,iVBORw0KGgo=)")).toEqual([]);
  });

  it("inline colour and highlight spans, which round-trip by design", () => {
    const md = 'a <span style="color:#3182ce">blue</span> and <mark>hl</mark> b';
    expect(ids(md)).toEqual([]);
  });

  it("indented continuation lines inside a list", () => {
    const md = "- first item\n\n    continued paragraph of the item\n\n- second";
    expect(ids(md)).not.toContain("indented-code");
  });

  it("a link whose text contains a colon", () => {
    expect(ids("[see: this](https://example.com)")).not.toContain("reference-links");
  });

  it("a list using dashes, which is not a setext underline", () => {
    expect(ids("intro\n\n- one\n- two")).not.toContain("setext");
  });

  it("empty input", () => {
    expect(ids("")).toEqual([]);
  });
});
