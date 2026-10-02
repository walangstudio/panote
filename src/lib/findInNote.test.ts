// @vitest-environment happy-dom
import { describe, it, expect } from "vitest";
import { matchOffsets, stepMatch, matchLabel, collectMatches } from "./findInNote";

describe("matchOffsets", () => {
  it("finds every occurrence, ignoring case", () => {
    expect(matchOffsets("Apple apple APPLE", "apple")).toEqual([0, 6, 12]);
  });

  it("does not overlap matches", () => {
    expect(matchOffsets("aaaa", "aa")).toEqual([0, 2]);
  });

  it("treats the query as text, not a pattern", () => {
    expect(matchOffsets("a.b axb (c)", ".")).toEqual([1]);
    expect(matchOffsets("a.b axb (c)", "(c)")).toEqual([8]);
  });

  it("finds nothing for an empty query", () => {
    expect(matchOffsets("abc", "")).toEqual([]);
  });
});

describe("stepMatch", () => {
  it("moves forward and back, wrapping at both ends", () => {
    expect(stepMatch(0, 3, 1)).toBe(1);
    expect(stepMatch(2, 3, 1)).toBe(0);
    expect(stepMatch(0, 3, -1)).toBe(2);
  });

  it("stays put with nothing to step through", () => {
    expect(stepMatch(0, 0, 1)).toBe(0);
  });
});

describe("matchLabel", () => {
  it("counts from one", () => {
    expect(matchLabel(2, 7, "x")).toBe("3 of 7");
  });

  it("says when nothing matched, and nothing before a query", () => {
    expect(matchLabel(0, 0, "x")).toBe("No matches");
    expect(matchLabel(0, 0, "")).toBe("");
  });
});

describe("collectMatches", () => {
  it("walks text and field values in document order", () => {
    const root = document.createElement("div");
    root.innerHTML = `<textarea>Cat title</textarea><p>a cat and a <b>CAT</b></p><input value="no" /><input value="cat" /><input type="checkbox" value="cat" />`;
    document.body.appendChild(root);
    const found = collectMatches(root, "cat");
    expect(found.map(m => m.kind)).toEqual(["field", "range", "range", "field"]);
    const [first, second, , last] = found;
    expect(first.kind === "field" && [first.start, first.end]).toEqual([0, 3]);
    expect(second.kind === "range" && second.range.toString()).toBe("cat");
    expect(last.kind === "field" && last.el.value).toBe("cat");
    root.remove();
  });

  it("skips what is excluded", () => {
    const root = document.createElement("div");
    root.innerHTML = `<div class="bar"><input value="cat" /> cat</div><p>cat</p>`;
    document.body.appendChild(root);
    expect(collectMatches(root, "cat", root.querySelector(".bar")).length).toBe(1);
    root.remove();
  });
});
