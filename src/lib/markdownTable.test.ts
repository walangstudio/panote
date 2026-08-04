import { describe, it, expect } from "vitest";
import { escapeCell } from "./markdownTable";

// A cell is delimited by pipes and newlines, so both have to survive a
// round-trip as literal text. The backslash is the one that bites: escape the
// pipe without escaping the backslash first and the escape character itself
// gets consumed, freeing the pipe to split the row.
describe("escapeCell", () => {
  it("escapes a pipe so it cannot end the cell", () => {
    expect(escapeCell("a|b")).toBe("a\\|b");
  });

  it("escapes the backslash before the pipe, not after", () => {
    // `a\|b` must not become `a\\|b` - that reads back as an escaped backslash
    // followed by a live separator.
    expect(escapeCell("a\\|b")).toBe("a\\\\\\|b");
  });

  it("escapes a lone backslash", () => {
    expect(escapeCell("C:\\Users")).toBe("C:\\\\Users");
  });

  it("flattens newlines, which would otherwise end the row", () => {
    expect(escapeCell("one\ntwo")).toBe("one two");
    expect(escapeCell("one\r\ntwo")).toBe("one two");
  });

  it("leaves ordinary text alone", () => {
    expect(escapeCell("plain text")).toBe("plain text");
  });
});
