import StarterKit from "@tiptap/starter-kit";
import { TaskList, TaskItem } from "@tiptap/extension-list";
import { TextStyle, Color } from "@tiptap/extension-text-style";
import Highlight from "@tiptap/extension-highlight";
import { Placeholder } from "@tiptap/extensions";
import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import Image from "@tiptap/extension-image";
import { createLowlight, common } from "lowlight";
import { Markdown } from "tiptap-markdown";
import { tableExtensions } from "$lib/markdownTable";

/// The editor's extension list, and with it the ProseMirror schema.
///
/// SECURITY: this schema is an allowlist. Imported and transferred notes are
/// untrusted, and TipTap builds the document by matching parsed HTML against
/// these node/mark specs — anything unrecognised cannot be constructed, so no
/// script, event handler or unknown element survives. That is why the app has
/// no `{@html}` sink. Adding an extension widens the allowlist: review it.
///
/// DATA: it is also the set of things a note can round-trip. markdown-it parses
/// a SUPERSET of this (tables, images, raw HTML, front matter, reference links);
/// anything parsed but absent here is dropped to its text and then saved over.
/// Tables and images are registered for exactly that reason, not to author them.
export function createExtensions(placeholder = "Start writing…") {
  return [
    // codeBlock off: replaced by the lowlight-highlighted variant below.
    // Link keeps its default isAllowedUri — that guard is load-bearing for raw
    // HTML anchors in imported notes (markdown-it's own validateLink only
    // covers the markdown link syntax). Verified by the scheme tests.
    StarterKit.configure({ codeBlock: false }),
    CodeBlockLowlight.configure({ lowlight: createLowlight(common) }),
    ...tableExtensions,
    // allowBase64 covers the data: URIs the app produces. Remote sources are
    // additionally blocked by the CSP (img-src 'self' data: blob:), so an
    // imported note cannot use an image to phone home.
    Image.configure({ allowBase64: true, inline: false }),
    TaskList,
    TaskItem.configure({ nested: true }),
    TextStyle,
    Color,
    Highlight.configure({ multicolor: true }),
    Placeholder.configure({ placeholder }),
    // html:true lets colour and highlight survive a round trip — markdown has no
    // syntax for them, so they serialise as inline spans. Safe because the schema
    // above, not the HTML string, decides what becomes a node.
    Markdown.configure({
      html: true,
      transformPastedText: true,
      transformCopiedText: true,
      linkify: true,
      breaks: false,
    }),
  ];
}
