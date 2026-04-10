import { describe, it, expect } from "vitest";
import { detectFormat } from "./detectFormat";

describe("detectFormat", () => {
  it("returns plain for empty string", () => {
    expect(detectFormat("")).toBe("plain");
  });

  it("returns plain for simple text", () => {
    expect(detectFormat("Just some notes")).toBe("plain");
  });

  it("returns code for fenced code block", () => {
    expect(detectFormat("Some text\n```rust\nfn main() {}\n```")).toBe("code");
  });

  it("returns markdown for heading", () => {
    expect(detectFormat("# Title\nContent")).toBe("markdown");
  });

  it("returns markdown for bold", () => {
    expect(detectFormat("This has **bold** text")).toBe("markdown");
  });

  it("returns markdown for link", () => {
    expect(detectFormat("See [docs](https://example.com)")).toBe("markdown");
  });

  it("returns markdown for unordered list", () => {
    expect(detectFormat("- item one\n- item two")).toBe("markdown");
  });

  it("returns markdown for ordered list", () => {
    expect(detectFormat("1. first\n2. second")).toBe("markdown");
  });

  it("returns markdown for blockquote", () => {
    expect(detectFormat("> quoted text")).toBe("markdown");
  });

  it("prioritizes code over markdown", () => {
    expect(detectFormat("# Title\n```js\nconsole.log('hi')\n```")).toBe("code");
  });
});
