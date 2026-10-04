// @vitest-environment happy-dom
//
// The editor parses a note's markdown and re-serialises it on every edit, so any
// construct the schema cannot represent is silently rewritten and then saved
// over. That already destroyed real notes.
//
// These drive the parse→serialise path directly rather than through the UI: the
// editor is built from the SAME extension list the component uses
// ($lib/editorExtensions), so what is asserted here is what ships.
import { describe, it, expect, afterEach } from "vitest";
import { Editor } from "@tiptap/core";
import type { MarkdownStorage } from "tiptap-markdown";
import { createExtensions } from "$lib/editorExtensions";

let editor: Editor | null = null;

afterEach(() => {
  editor?.destroy();
  editor = null;
  document.body.innerHTML = "";
});

/// Parse markdown into the real schema, then serialise it back — exactly what
/// happens between opening a note and saving it.
function roundTrip(body: string): string {
  const element = document.createElement("div");
  document.body.appendChild(element);
  editor = new Editor({ element, extensions: createExtensions(), content: body });
  return (editor.storage as unknown as { markdown: MarkdownStorage }).markdown.getMarkdown();
}

describe("markdown round-trip", () => {
  it("keeps a GFM table as a pipe table", () => {
    const out = roundTrip("| a | b |\n| --- | --- |\n| 1 | 2 |");
    expect(out).toContain("| a | b |");
    expect(out).toContain("| 1 | 2 |");
    // Must stay markdown, not degrade to tiptap-markdown's HTMLNode fallback.
    expect(out).not.toContain("<table");
  });

  it("does not flatten table cells into a single paragraph", () => {
    const out = roundTrip("| x | y |\n| --- | --- |\n| 1 | 2 |");
    expect(out).not.toMatch(/^x\s+y\s+1\s+2$/m);
  });

  it("preserves inline formatting inside a cell", () => {
    const out = roundTrip("| h |\n| --- |\n| **bold** |");
    expect(out).toContain("**bold**");
  });

  it("escapes a literal pipe so it cannot break the row", () => {
    const out = roundTrip("| h |\n| --- |\n| a \\| b |");
    expect(out).toContain("\\|");
  });

  it("keeps an image instead of dropping it", () => {
    const out = roundTrip("![alt text](data:image/png;base64,iVBORw0KGgo=)");
    expect(out).toContain("data:image/png;base64,iVBORw0KGgo=");
    expect(out).toContain("alt text");
  });

  it("keeps headings, lists, quotes and code fences", () => {
    const out = roundTrip("# Title\n\n- one\n- two\n\n> quoted\n\n```js\nlet x = 1;\n```");
    expect(out).toContain("# Title");
    expect(out).toContain("- one");
    expect(out).toContain("> quoted");
    expect(out).toContain("```js");
  });

  it("keeps task list state", () => {
    const out = roundTrip("- [x] done\n- [ ] todo");
    expect(out).toMatch(/\[x\] done/);
    expect(out).toMatch(/\[ \] todo/);
  });

  it("keeps a colour span, which markdown cannot express natively", () => {
    const out = roundTrip('a <span style="color:#3182ce">blue</span> b');
    expect(out).toContain("#3182ce");
    expect(out).toContain("blue");
  });

  it("keeps ordinary emphasis and links", () => {
    const out = roundTrip("**b** and *i* and [x](https://example.com)");
    expect(out).toContain("**b**");
    expect(out).toContain("https://example.com");
  });
});

// The schema is an allowlist: untrusted markdown from an import or a LAN
// transfer is matched against it, so unknown elements cannot be constructed.
// These are the payloads such a note could realistically carry.
describe("untrusted markup cannot smuggle script", () => {
  const hostile = [
    ["script tag", "<script>window.__pwned = 1</script>"],
    ["img onerror", '<img src=x onerror="window.__pwned=1">'],
    ["svg onload", '<svg onload="window.__pwned=1"></svg>'],
    // A relative src: happy-dom really does try to fetch an absolute one, and an
    // offline-first project's test suite should not reach for the network.
    ["iframe", '<iframe src="/evil-frame"></iframe>'],
    ["object tag", '<object data="evil.swf"></object>'],
    ["embed tag", '<embed src="evil.swf">'],
    ["body onload", '<body onload="window.__pwned=1">'],
    ["mathml annotation-xml", '<math><annotation-xml encoding="text/html"><script>window.__pwned=1</script></annotation-xml></math>'],
    ["style block", "<style>body{background:url(evil)}</style>"],
    ["meta refresh", '<meta http-equiv="refresh" content="0;url=/evil-redirect">'],
  ] as const;

  for (const [name, payload] of hostile) {
    it(`neutralises ${name}`, () => {
      const out = roundTrip(payload);
      expect(document.querySelector("script")).toBeNull();
      expect(document.querySelector("iframe")).toBeNull();
      expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();
      expect(out.toLowerCase()).not.toContain("onerror=");
      expect(out.toLowerCase()).not.toContain("onload=");
      expect(out.toLowerCase()).not.toContain("<script");
    });
  }
});

// Scheme-based payloads: a link or image whose URL is itself executable. The
// schema permits link and image NODES, so the defence has to be at the URL.
//
// The property asserted is "nothing executable was CONSTRUCTED" — not "the
// string is absent". Verified behaviour: TipTap refuses to build a link/image
// for these schemes and the payload survives as inert paragraph text. Keeping
// that text is correct; deleting a user's characters would be data loss.
describe("executable URL schemes never become live links", () => {
  const schemes = [
    ["javascript: link", "[click me](javascript:window.__pwned=1)"],
    ["JaVaScRiPt: link (case)", "[click me](JaVaScRiPt:window.__pwned=1)"],
    ["vbscript: link", "[click me](vbscript:msgbox(1))"],
    ["javascript: image", "![x](javascript:window.__pwned=1)"],
    ["javascript: raw anchor", '<a href="javascript:window.__pwned=1">click</a>'],
    ["data:text/html link", "[click](data:text/html;base64,PHNjcmlwdD4x)"],
  ] as const;

  const EXECUTABLE = /^\s*(javascript|vbscript|data:text\/html)/i;

  for (const [name, payload] of schemes) {
    it(`refuses to build a live node for ${name}`, () => {
      roundTrip(payload);
      expect((window as unknown as { __pwned?: number }).__pwned).toBeUndefined();

      for (const a of document.querySelectorAll("a")) {
        expect(a.getAttribute("href") ?? "").not.toMatch(EXECUTABLE);
      }
      for (const img of document.querySelectorAll("img")) {
        expect(img.getAttribute("src") ?? "").not.toMatch(EXECUTABLE);
      }
    });
  }

  it("keeps the payload as inert text rather than deleting the user's characters", () => {
    roundTrip("[click me](javascript:window.__pwned=1)");
    expect(document.querySelectorAll("a")).toHaveLength(0);
    expect(document.body.textContent).toContain("click me");
  });
});
