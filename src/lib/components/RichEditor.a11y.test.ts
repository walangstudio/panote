// @vitest-environment happy-dom
//
// The toolbar only ever toggled a CSS `.on` class, so a screen-reader user had
// no way to tell Bold was active. Worse, the icon-only buttons relied on
// `title` for a name, but the Material Symbols span's raw ligature text
// ("format_bold") wins over `title` in the accname algorithm, so buttons
// announced the icon identifier instead of their purpose.
import { describe, it, expect, afterEach } from "vitest";
import { mount, unmount } from "svelte";
import { readFileSync } from "fs";
import RichEditor from "./RichEditor.svelte";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

async function setup(body = "hello") {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const content = { body };
  const app = mount(RichEditor, { target, props: { content, editable: true } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  await new Promise(r => setTimeout(r, 0));
  return { target, content };
}

function byTitle(target: HTMLElement, title: string) {
  const btn = target.querySelector<HTMLButtonElement>(`button[title="${title}"]`);
  if (!btn) throw new Error(`no toolbar button titled "${title}"`);
  return btn;
}

describe("RichEditor toolbar accessibility", () => {
  it("gives icon-only buttons an aria-label independent of the icon ligature text", async () => {
    const { target } = await setup();
    const bold = byTitle(target, "Bold");
    expect(bold.getAttribute("aria-label")).toBe("Bold");
  });

  it("hides the icon span from the accessible name", async () => {
    const { target } = await setup();
    const bold = byTitle(target, "Bold");
    const icon = bold.querySelector(".material-symbols-outlined");
    expect(icon?.getAttribute("aria-hidden")).toBe("true");
  });

  it("reflects toggle state via aria-pressed", async () => {
    // A block-level command, not a mark: toggling bold/italic on a collapsed
    // cursor only sets a "stored mark" for the next typed character and
    // deliberately doesn't touch the document, so it wouldn't move the
    // existing `.on` indicator either. Bullet list flips the block itself,
    // which is the case aria-pressed needs to track.
    const { target, content } = await setup("hello");
    const bulletList = byTitle(target, "Bullet list");
    expect(bulletList.getAttribute("aria-pressed")).toBe("false");
    bulletList.click();
    await new Promise(r => setTimeout(r, 0));
    expect(content.body.trim()).toBe("- hello");
    expect(bulletList.getAttribute("aria-pressed")).toBe("true");
  });

  it("reports open/closed state on dropdown triggers via aria-expanded", async () => {
    const { target } = await setup();
    const heading = byTitle(target, "Heading");
    expect(heading.getAttribute("aria-expanded")).toBe("false");
    heading.click();
    await new Promise(r => setTimeout(r, 0));
    expect(heading.getAttribute("aria-expanded")).toBe("true");
  });

  // happy-dom doesn't lay out or resolve a component's scoped <style> block,
  // so the touch-target size is asserted straight off the source rule instead
  // of a mounted element's computed style.
  it("gives toolbar buttons at least a 44px touch target", () => {
    const src = readFileSync("src/lib/components/RichEditor.svelte", "utf-8");
    const rule = src.match(/\.fmt-btn\s*\{([^}]*)\}/)?.[1] ?? "";
    const width = Number(rule.match(/(?:min-)?width:\s*(\d+(?:\.\d+)?)px/)?.[1]);
    const height = Number(rule.match(/(?:min-)?height:\s*(\d+(?:\.\d+)?)px/)?.[1]);
    expect(width).toBeGreaterThanOrEqual(44);
    expect(height).toBeGreaterThanOrEqual(44);
  });
});
