export type ContentFormat = "plain" | "markdown" | "code";

const CODE_FENCE = /^```\w/m;
const MD_HEADING = /^#{1,6}\s/m;
const MD_BOLD_ITALIC = /(\*\*|__).+?\1|(\*|_).+?\2/;
const MD_LINK = /\[.+?\]\(.+?\)/;
const MD_LIST = /^(\s*[-*+]|\s*\d+\.)\s/m;
const MD_BLOCKQUOTE = /^>\s/m;

export function detectFormat(body: string): ContentFormat {
  if (CODE_FENCE.test(body)) return "code";
  if (
    MD_HEADING.test(body) ||
    MD_BOLD_ITALIC.test(body) ||
    MD_LINK.test(body) ||
    MD_LIST.test(body) ||
    MD_BLOCKQUOTE.test(body)
  )
    return "markdown";
  return "plain";
}

export const formatMeta: Record<ContentFormat, { icon: string; label: string; color: string }> = {
  plain: { icon: "description", label: "Plain text", color: "accent" },
  markdown: { icon: "edit_note", label: "Markdown", color: "secondary" },
  code: { icon: "code", label: "Code", color: "secondary" },
};
