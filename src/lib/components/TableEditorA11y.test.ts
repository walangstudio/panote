// @vitest-environment happy-dom
//
// Keyboard and screen-reader access to masked (credential) cells. A mouse user
// can reveal and copy a secret; these assert a keyboard-only user can too, and
// that a screen reader is told the value is hidden rather than read eight
// bullet characters.
import { describe, it, expect, afterEach } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { mount, unmount } from "svelte";
import TableEditor from "./TableEditor.svelte";
import type { TableContent } from "$lib/tableParsers";

const SECRET = "hunter2-super-secret";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

function contentWithSecret(): TableContent {
  return {
    columns: [
      { id: "site", name: "Site" },
      { id: "pw", name: "Password", type: "masked" },
    ],
    rows: [{ id: "r1", cells: { site: "https://github.com", pw: SECRET } }],
  };
}

async function setup(content: TableContent) {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(TableEditor, { target, props: { content } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  await new Promise(r => setTimeout(r, 0));
  return { target };
}

const buttonByLabel = (t: HTMLElement, label: string) =>
  t.querySelector<HTMLElement>(`button[aria-label="${label}"]`);

/// Press a key the way a browser would: the event bubbles from the focused
/// element, and a handler that preventDefaults it kills the element's own
/// default activation.
function press(el: HTMLElement, key: string) {
  const e = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
  el.dispatchEvent(e);
  return e;
}

/// Text a screen reader would actually announce: aria-hidden subtrees removed.
function accessibleText(el: Element): string {
  return Array.from(el.childNodes)
    .map((n) => {
      if (n.nodeType === 3) return n.textContent ?? "";
      if (n.nodeType !== 1) return "";
      const e = n as Element;
      return e.getAttribute("aria-hidden") === "true" ? "" : accessibleText(e);
    })
    .join(" ")
    .replace(/\s+/g, " ")
    .trim();
}

describe("TableEditor masked cells: keyboard", () => {
  it("reveals with Enter instead of opening the row modal", async () => {
    const { target } = await setup(contentWithSecret());
    const e = press(buttonByLabel(target, "Show value")!, "Enter");
    await new Promise(r => setTimeout(r, 0));

    // preventDefault here would cancel the button's own activation
    expect(e.defaultPrevented).toBe(false);
    expect(target.querySelector('[role="dialog"]')).toBeNull();
  });

  it("copies with Space instead of opening the row modal", async () => {
    const { target } = await setup(contentWithSecret());
    const e = press(buttonByLabel(target, "Copy value")!, " ");
    await new Promise(r => setTimeout(r, 0));

    expect(e.defaultPrevented).toBe(false);
    expect(target.querySelector('[role="dialog"]')).toBeNull();
  });

  it("follows a URL cell link with Enter instead of opening the row modal", async () => {
    const { target } = await setup(contentWithSecret());
    const e = press(target.querySelector<HTMLElement>(".url-link")!, "Enter");
    await new Promise(r => setTimeout(r, 0));

    expect(e.defaultPrevented).toBe(false);
    expect(target.querySelector('[role="dialog"]')).toBeNull();
  });

  it("still opens the row modal on Enter on the row itself", async () => {
    const { target } = await setup(contentWithSecret());
    const e = press(target.querySelector<HTMLElement>("tr.row-clickable")!, "Enter");
    await new Promise(r => setTimeout(r, 0));

    expect(e.defaultPrevented).toBe(true);
    expect(target.querySelector('[role="dialog"]')).not.toBeNull();
  });
});

describe("TableEditor masked cells: screen reader", () => {
  it("announces that the value is hidden, not eight bullets", async () => {
    const { target } = await setup(contentWithSecret());
    const cell = target.querySelector(".secret-cell")!;

    expect(accessibleText(cell)).not.toContain("•");
    expect(accessibleText(cell)).toMatch(/hidden/i);
  });

  it("announces the value and its revealed state once shown", async () => {
    const { target } = await setup(contentWithSecret());
    buttonByLabel(target, "Show value")!.click();
    await new Promise(r => setTimeout(r, 0));
    const cell = target.querySelector(".secret-cell")!;

    expect(accessibleText(cell)).toContain(SECRET);
    expect(accessibleText(cell)).toMatch(/shown|revealed/i);
    expect(accessibleText(cell)).not.toMatch(/hidden/i);
  });
});

describe("TableEditor masked cells: touch targets", () => {
  // Component CSS is never injected into the test DOM (vitest css:false), so
  // getComputedStyle is useless here — assert against the source rule instead.
  it("gives the reveal/copy buttons a 44px hit area", () => {
    const here = dirname(fileURLToPath(import.meta.url));
    const src = readFileSync(join(here, "TableEditor.svelte"), "utf8");
    const style = src.slice(src.indexOf("<style>"));
    const px = (selector: string, prop: string) => {
      const body = style.match(new RegExp(`\\${selector}\\s*\\{([^}]*)\\}`))?.[1] ?? "";
      return Number(body.match(new RegExp(`${prop}:\\s*([\\d.]+)px`))?.[1] ?? 0);
    };
    // Either the button itself is touch-sized, or it carries an overlaid target.
    const hit = (prop: string) =>
      Math.max(px(".secret-btn", prop), px(".secret-btn::before", prop));

    expect(hit("width")).toBeGreaterThanOrEqual(44);
    expect(hit("height")).toBeGreaterThanOrEqual(44);
  });
});
