// @vitest-environment happy-dom
//
// The three-step import flow: pick a format, paste, map columns, import. The
// mapping step is what makes this risky — rows are keyed by the *parsed* column
// name and have to come out keyed by the *table's* column id, and a parser that
// marks a column secret has to carry that through so an imported password
// arrives already masked rather than in the clear.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import type { TableColumn } from "$lib/tableParsers";
import TableImportModal from "./TableImportModal.svelte";

const columns: TableColumn[] = [
  { id: "c-name", name: "Name", type: "text" },
  { id: "c-age", name: "Age", type: "text" },
  { id: "c-secret", name: "Password", type: "text" },
] as TableColumn[];

let cleanup: (() => void) | null = null;
const flush = async () => { await Promise.resolve(); await new Promise(r => setTimeout(r, 0)); };

function setup(cols: TableColumn[] = columns) {
  const onimport = vi.fn();
  const onclose = vi.fn();
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(TableImportModal, { target, props: { columns: cols, onimport, onclose } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target, onimport, onclose };
}

const formatCard = (t: HTMLElement, name: RegExp) =>
  [...t.querySelectorAll<HTMLButtonElement>(".format-card")]
    .find(b => name.test(b.querySelector(".format-name")?.textContent ?? ""))!;

const primary = (t: HTMLElement) => t.querySelector<HTMLButtonElement>(".btn-primary")!;

async function paste(t: HTMLElement, text: string) {
  const ta = t.querySelector<HTMLTextAreaElement>(".data-textarea")!;
  ta.value = text;
  ta.dispatchEvent(new Event("input", { bubbles: true }));
  await flush();
}

/// Format name -> paste -> Parse, landing on the preview step.
async function parseWith(t: HTMLElement, format: RegExp, text: string) {
  formatCard(t, format).click();
  await flush();
  await paste(t, text);
  primary(t).click();
  await flush();
}

const mappingFor = (t: HTMLElement, parsedCol: string) =>
  [...t.querySelectorAll<HTMLElement>(".mapping-row")]
    .find(r => r.querySelector(".parsed-col")?.textContent?.trim() === parsedCol)
    ?.querySelector<HTMLSelectElement>(".col-select");

async function mapTo(t: HTMLElement, parsedCol: string, colId: string) {
  const sel = mappingFor(t, parsedCol)!;
  sel.value = colId;
  sel.dispatchEvent(new Event("change", { bubbles: true }));
  await flush();
}

beforeEach(() => vi.clearAllMocks());
afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

describe("choosing a format", () => {
  it("offers the built-in parsers", () => {
    const { target } = setup();
    const names = [...target.querySelectorAll(".format-name")].map(e => e.textContent);
    expect(names.join(" ")).toMatch(/CSV/i);
    expect(names.join(" ")).toMatch(/env/i);
  });

  it("moves to the paste step once one is picked", async () => {
    const { target } = setup();
    formatCard(target, /CSV/i).click();
    await flush();
    expect(target.querySelector(".data-textarea")).toBeTruthy();
  });

  it("will not parse an empty paste", async () => {
    const { target } = setup();
    formatCard(target, /CSV/i).click();
    await flush();
    expect(primary(target).disabled).toBe(true);
  });

  it("goes back to the format list", async () => {
    const { target } = setup();
    formatCard(target, /CSV/i).click();
    await flush();
    target.querySelector<HTMLButtonElement>(".btn-cancel")!.click();
    await flush();
    expect(target.querySelectorAll(".format-card").length).toBeGreaterThan(0);
  });
});

describe("parsing", () => {
  it("previews parsed rows and counts them", async () => {
    const { target } = setup();
    await parseWith(target, /CSV/i, "Name,Age\nAlice,30\nBob,25");

    expect(target.querySelector(".row-count")!.textContent).toContain("2 rows");
    expect(target.querySelector(".preview-table")!.textContent).toContain("Alice");
    expect(target.querySelector(".preview-table")!.textContent).toContain("Bob");
  });

  it("uses the singular for one row", async () => {
    const { target } = setup();
    await parseWith(target, /CSV/i, "Name,Age\nAlice,30");
    expect(target.querySelector(".row-count")!.textContent).toContain("1 row");
    expect(target.querySelector(".row-count")!.textContent).not.toContain("1 rows");
  });

  it("says so rather than showing an empty preview", async () => {
    const { target } = setup();
    formatCard(target, /CSV/i).click();
    await flush();
    await paste(target, "Name,Age");   // header only, no data rows
    primary(target).click();
    await flush();

    expect(target.querySelector(".error")!.textContent).toContain("No rows parsed");
    expect(target.querySelector(".preview-table")).toBeNull();
  });

  it("caps the preview at 15 rows but reports the true total", async () => {
    const { target } = setup();
    const rows = Array.from({ length: 20 }, (_, i) => `User${i},${i}`).join("\n");
    await parseWith(target, /CSV/i, `Name,Age\n${rows}`);

    expect(target.querySelectorAll(".preview-table tbody tr")).toHaveLength(15);
    expect(target.querySelector(".preview-note")!.textContent).toContain("15 of 20");
  });

  it("goes back to the paste step to fix the input", async () => {
    const { target } = setup();
    await parseWith(target, /CSV/i, "Name,Age\nAlice,30");
    target.querySelector<HTMLButtonElement>(".btn-cancel")!.click();
    await flush();
    expect(target.querySelector(".data-textarea")).toBeTruthy();
  });
});

describe("column mapping", () => {
  it("auto-maps parsed columns whose names match, case-insensitively", async () => {
    const { target } = setup();
    await parseWith(target, /CSV/i, "name,age\nAlice,30");

    expect(mappingFor(target, "name")!.value).toBe("c-name");
    expect(mappingFor(target, "age")!.value).toBe("c-age");
  });

  it("leaves an unrecognised column unmapped rather than guessing", async () => {
    const { target } = setup();
    await parseWith(target, /CSV/i, "name,nickname\nAlice,Al");
    expect(mappingFor(target, "nickname")!.value).toBe("");
  });

  it("refuses to import when nothing is mapped", async () => {
    const { target } = setup();
    await parseWith(target, /CSV/i, "foo,bar\n1,2");
    expect(primary(target).disabled).toBe(true);
  });

  it("emits rows keyed by the table's column ids, not the parsed names", async () => {
    const { target, onimport } = setup();
    await parseWith(target, /CSV/i, "name,age\nAlice,30\nBob,25");
    primary(target).click();
    await flush();

    expect(onimport).toHaveBeenCalledTimes(1);
    expect(onimport.mock.calls[0][0]).toEqual([
      { "c-name": "Alice", "c-age": "30" },
      { "c-name": "Bob", "c-age": "25" },
    ]);
  });

  it("drops columns the user chose to skip", async () => {
    const { target, onimport } = setup();
    await parseWith(target, /CSV/i, "name,age\nAlice,30");
    await mapTo(target, "age", "");
    primary(target).click();
    await flush();

    expect(onimport.mock.calls[0][0]).toEqual([{ "c-name": "Alice" }]);
  });

  it("honours a column remapped by hand", async () => {
    const { target, onimport } = setup();
    await parseWith(target, /CSV/i, "name,nickname\nAlice,Al");
    await mapTo(target, "nickname", "c-age");
    primary(target).click();
    await flush();

    expect(onimport.mock.calls[0][0]).toEqual([{ "c-name": "Alice", "c-age": "Al" }]);
  });
});

// An imported credential must not land in the table as visible text.
describe("secret columns", () => {
  it("marks the parser's secret column as masked on the way in", async () => {
    const { target, onimport } = setup();
    await parseWith(target, /env/i, "NAME=Alice\nPASSWORD=hunter2");

    const valueCol = [...target.querySelectorAll(".parsed-col")]
      .map(e => e.textContent!.trim())
      .find(n => /value/i.test(n));
    expect(valueCol, "env parser should produce a value column").toBeTruthy();
    await mapTo(target, valueCol!, "c-secret");

    primary(target).click();
    await flush();

    expect(onimport).toHaveBeenCalledTimes(1);
    expect(onimport.mock.calls[0][1]).toContain("c-secret");
  });

  it("reports no masked ids for a format that has no secrets", async () => {
    const { target, onimport } = setup();
    await parseWith(target, /CSV/i, "name,age\nAlice,30");
    primary(target).click();
    await flush();
    expect(onimport.mock.calls[0][1]).toEqual([]);
  });

  it("does not mask a secret column the user skipped", async () => {
    const { target, onimport } = setup();
    await parseWith(target, /env/i, "NAME=Alice\nPASSWORD=hunter2");

    for (const row of target.querySelectorAll<HTMLElement>(".mapping-row")) {
      const name = row.querySelector(".parsed-col")!.textContent!.trim();
      await mapTo(target, name, /key/i.test(name) ? "c-name" : "");
    }
    primary(target).click();
    await flush();

    expect(onimport.mock.calls[0][1]).toEqual([]);
  });
});

describe("custom regex", () => {
  const openRegex = async (t: HTMLElement) => {
    formatCard(t, /Custom Regex/i).click();
    await flush();
  };
  const fill = async (t: HTMLElement, pattern: string, cols: string) => {
    const inputs = t.querySelectorAll<HTMLInputElement>(".modal input[type='text'], .modal input:not([type])");
    inputs[0].value = pattern;
    inputs[0].dispatchEvent(new Event("input", { bubbles: true }));
    inputs[1].value = cols;
    inputs[1].dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
  };

  it("will not continue without both a pattern and columns", async () => {
    const { target } = setup();
    await openRegex(target);
    expect(primary(target).disabled).toBe(true);
  });

  it("parses with a hand-written pattern", async () => {
    const { target, onimport } = setup();
    await openRegex(target);
    await fill(target, "^(?<name>\\w+):(?<age>\\d+)$", "name, age");
    primary(target).click();
    await flush();

    await paste(target, "Alice:30\nBob:25");
    primary(target).click();
    await flush();

    expect(target.querySelector(".row-count")!.textContent).toContain("2 rows");
    primary(target).click();
    await flush();
    expect(onimport.mock.calls[0][0]).toEqual([
      { "c-name": "Alice", "c-age": "30" },
      { "c-name": "Bob", "c-age": "25" },
    ]);
  });
});
