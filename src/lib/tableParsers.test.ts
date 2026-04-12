import { describe, it, expect } from "vitest";
import {
  csvParser,
  psvParser,
  jsonParser,
  kvParser,
  urlDescParser,
  makeCustomParser,
  slugifyColumn,
} from "./tableParsers";

// ---- CSV ----

describe("csvParser", () => {
  it("parses basic CSV with header auto-detection", () => {
    const result = csvParser.parse("name,age\nAlice,30\nBob,25");
    expect(result.columns).toEqual(["name", "age"]);
    expect(result.rows).toEqual([
      { name: "Alice", age: "30" },
      { name: "Bob", age: "25" },
    ]);
  });

  it("handles quoted values with commas", () => {
    const result = csvParser.parse('name,desc\nAlice,"hello, world"\nBob,simple', { hasHeader: true });
    expect(result.rows[0].desc).toBe("hello, world");
    expect(result.rows[1].desc).toBe("simple");
  });

  it("handles escaped quotes in quoted fields", () => {
    const result = csvParser.parse('a\n"say ""hi"""', { hasHeader: true });
    expect(result.rows[0].a).toBe('say "hi"');
  });

  it("uses generated column names without header", () => {
    const result = csvParser.parse("1,2,3\n4,5,6", { hasHeader: false });
    expect(result.columns).toEqual(["col1", "col2", "col3"]);
    expect(result.rows).toHaveLength(2);
  });

  it("forces header when hasHeader is true", () => {
    const result = csvParser.parse("a,b\n1,2", { hasHeader: true });
    expect(result.columns).toEqual(["a", "b"]);
    expect(result.rows).toEqual([{ a: "1", b: "2" }]);
  });

  it("returns empty for empty input", () => {
    const result = csvParser.parse("");
    expect(result.columns).toEqual([]);
    expect(result.rows).toEqual([]);
  });
});

// ---- PSV ----

describe("psvParser", () => {
  it("parses pipe-separated values", () => {
    const result = psvParser.parse("name|age\nAlice|30\nBob|25");
    expect(result.columns).toEqual(["name", "age"]);
    expect(result.rows).toEqual([
      { name: "Alice", age: "30" },
      { name: "Bob", age: "25" },
    ]);
  });

  it("trims whitespace around pipes", () => {
    const result = psvParser.parse("a | b\n 1 | 2 ");
    expect(result.columns).toEqual(["a", "b"]);
    expect(result.rows[0]).toEqual({ a: "1", b: "2" });
  });

  it("returns empty for empty input", () => {
    expect(psvParser.parse("").rows).toEqual([]);
  });
});

// ---- JSON ----

describe("jsonParser", () => {
  it("parses array of objects", () => {
    const result = jsonParser.parse('[{"a":"1","b":"2"},{"a":"3","b":"4"}]');
    expect(result.columns).toEqual(["a", "b"]);
    expect(result.rows).toEqual([
      { a: "1", b: "2" },
      { a: "3", b: "4" },
    ]);
  });

  it("coerces non-string values", () => {
    const result = jsonParser.parse('[{"x":42,"y":true,"z":null}]');
    expect(result.rows[0]).toEqual({ x: "42", y: "true", z: "" });
  });

  it("handles objects with different keys", () => {
    const result = jsonParser.parse('[{"a":"1"},{"b":"2"}]');
    expect(result.columns).toContain("a");
    expect(result.columns).toContain("b");
    expect(result.rows[0].b).toBe("");
    expect(result.rows[1].a).toBe("");
  });

  it("returns empty for invalid JSON", () => {
    const result = jsonParser.parse("not json");
    expect(result.columns).toEqual([]);
    expect(result.rows).toEqual([]);
  });

  it("returns empty for non-array JSON", () => {
    const result = jsonParser.parse('{"a":1}');
    expect(result.rows).toEqual([]);
  });
});

// ---- Key-Value ----

describe("kvParser", () => {
  it("parses key=value pairs", () => {
    const result = kvParser.parse("name=Alice\nage=30");
    expect(result.columns).toEqual(["name", "age"]);
    expect(result.rows).toEqual([{ name: "Alice", age: "30" }]);
  });

  it("parses key: value pairs", () => {
    const result = kvParser.parse("name: Alice\nage: 30");
    expect(result.columns).toEqual(["name", "age"]);
    expect(result.rows[0]).toEqual({ name: "Alice", age: "30" });
  });

  it("handles mixed separators", () => {
    const result = kvParser.parse("a=1\nb: 2");
    expect(result.rows[0]).toEqual({ a: "1", b: "2" });
  });

  it("skips non-matching lines", () => {
    const result = kvParser.parse("valid=yes\njust text\nalso=good");
    expect(result.columns).toEqual(["valid", "also"]);
  });

  it("returns empty for no matches", () => {
    const result = kvParser.parse("no pairs here");
    expect(result.columns).toEqual([]);
    expect(result.rows).toEqual([]);
  });
});

// ---- URL + Description (block-based) ----

describe("urlDescParser", () => {
  it("parses single URL with surrounding text as one row", () => {
    const input = "blah blah\nblah blah\n\n~/github.com/abc/def\n\ncxcxcx as";
    const result = urlDescParser.parse(input);
    expect(result.rows).toHaveLength(1);
    expect(result.rows[0].url).toBe("https://github.com/abc/def");
    expect(result.rows[0].desc).toBe("blah blah blah blah cxcxcx as");
  });

  it("parses multiple URLs into multiple rows", () => {
    const input = "first desc https://example.com/a second desc https://example.com/b third";
    const result = urlDescParser.parse(input);
    expect(result.rows).toHaveLength(2);
    expect(result.rows[0].url).toBe("https://example.com/a");
    expect(result.rows[0].desc).toBe("first desc");
    expect(result.rows[1].url).toBe("https://example.com/b");
    expect(result.rows[1].desc).toBe("second desc third");
  });

  it("normalizes ~/ prefix to https://", () => {
    const result = urlDescParser.parse("~/github.com/user/repo");
    expect(result.rows[0].url).toBe("https://github.com/user/repo");
  });

  it("normalizes -/ prefix to https://", () => {
    const result = urlDescParser.parse("-/github.com/user/repo");
    expect(result.rows[0].url).toBe("https://github.com/user/repo");
  });

  it("parses multiple -/ URLs into multiple rows", () => {
    const input = `desc one\n\n-/github.com/a/b\n\ndesc two\n\n-/github.com/c/d`;
    const result = urlDescParser.parse(input);
    expect(result.rows).toHaveLength(2);
    expect(result.rows[0].url).toBe("https://github.com/a/b");
    expect(result.rows[0].desc).toBe("desc one");
    expect(result.rows[1].url).toBe("https://github.com/c/d");
    expect(result.rows[1].desc).toBe("desc two");
  });

  it("handles URL-only input", () => {
    const result = urlDescParser.parse("https://example.com");
    expect(result.rows).toHaveLength(1);
    expect(result.rows[0].url).toBe("https://example.com");
    expect(result.rows[0].desc).toBe("");
  });

  it("handles no URLs — all text as desc", () => {
    const result = urlDescParser.parse("just some text\nwith lines");
    expect(result.rows).toHaveLength(1);
    expect(result.rows[0].url).toBe("");
    expect(result.rows[0].desc).toBe("just some text with lines");
  });

  it("handles empty input", () => {
    const result = urlDescParser.parse("");
    expect(result.rows).toEqual([]);
  });

  it("handles http:// URLs", () => {
    const result = urlDescParser.parse("desc http://example.com");
    expect(result.rows[0].url).toBe("http://example.com");
  });

  it("assigns pre-URL text as that URL's desc, trailing text to last URL", () => {
    const input = "desc A\n\n-/github.com/a/a\n\ndesc B\n\n-/github.com/b/b\n\ndesc C trailing\n\n-/github.com/c/c";
    const result = urlDescParser.parse(input);
    expect(result.rows).toHaveLength(3);
    expect(result.rows[0].desc).toBe("desc A");
    expect(result.rows[1].desc).toBe("desc B");
    expect(result.rows[2].desc).toBe("desc C trailing");
  });

  it("handles text only before URL", () => {
    const result = urlDescParser.parse("my project https://github.com/foo");
    expect(result.rows[0]).toEqual({ url: "https://github.com/foo", desc: "my project" });
  });

  it("handles text only after URL", () => {
    const result = urlDescParser.parse("https://github.com/foo my project");
    expect(result.rows[0]).toEqual({ url: "https://github.com/foo", desc: "my project" });
  });
});

// ---- Custom regex parser ----

describe("makeCustomParser", () => {
  it("creates parser from regex with named groups", () => {
    const parser = makeCustomParser({
      id: "date-event",
      name: "Date + Event",
      pattern: "(?<date>\\d{4}-\\d{2}-\\d{2})\\s+(?<event>.+)",
      columns: ["date", "event"],
    });
    const result = parser.parse("2024-01-15 Meeting with team");
    expect(result.columns).toEqual(["date", "event"]);
    expect(result.rows).toEqual([{ date: "2024-01-15", event: "Meeting with team" }]);
  });

  it("returns empty strings for non-matching lines", () => {
    const parser = makeCustomParser({
      id: "test",
      name: "Test",
      pattern: "(?<key>\\w+)=(?<val>\\w+)",
      columns: ["key", "val"],
    });
    const result = parser.parse("no match here");
    expect(result.rows).toEqual([{ key: "", val: "" }]);
  });
});

// ---- slugifyColumn ----

describe("slugifyColumn", () => {
  it("converts to lowercase with hyphens", () => {
    expect(slugifyColumn("My Column", [])).toBe("my-column");
  });

  it("strips leading/trailing hyphens", () => {
    expect(slugifyColumn("  -hello- ", [])).toBe("hello");
  });

  it("replaces non-alphanumeric chars", () => {
    expect(slugifyColumn("col@#$name", [])).toBe("col-name");
  });

  it("deduplicates with numeric suffix", () => {
    expect(slugifyColumn("name", ["name"])).toBe("name-2");
    expect(slugifyColumn("name", ["name", "name-2"])).toBe("name-3");
  });

  it("falls back to 'col' for empty input", () => {
    expect(slugifyColumn("", [])).toBe("col");
    expect(slugifyColumn("@#$", [])).toBe("col");
  });
});
