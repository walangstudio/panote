// @vitest-environment happy-dom
//
// These mount the real component and click the real toolbar buttons. A type
// check and a successful build both pass happily while every button is inert —
// which is exactly what shipped once — so the assertions here are on the note's
// markdown actually changing, not on the component merely rendering.
import { describe, it, expect, afterEach } from "vitest";
import { mount, unmount } from "svelte";
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
  // Let onMount construct the editor.
  await new Promise(r => setTimeout(r, 0));
  return { target, content };
}

function click(target: HTMLElement, title: string) {
  const btn = target.querySelector<HTMLButtonElement>(`button[title="${title}"]`);
  if (!btn) throw new Error(`no toolbar button titled "${title}"`);
  btn.click();
}

describe("RichEditor toolbar", () => {
  it("mounts an editing surface", async () => {
    const { target } = await setup();
    expect(target.querySelector(".tiptap")).toBeTruthy();
  });

  it("renders the formatting bar when editable", async () => {
    const { target } = await setup();
    expect(target.querySelectorAll(".format-bar button").length).toBeGreaterThan(10);
  });

  // Block-level commands act on the cursor's block, so they change the document
  // without needing a text selection — ideal for asserting dispatch really ran.
  it("bullet list turns the paragraph into a list item", async () => {
    const { target, content } = await setup("hello");
    click(target, "Bullet list");
    await new Promise(r => setTimeout(r, 0));
    expect(content.body.trim()).toBe("- hello");
  });

  it("numbered list turns the paragraph into an ordered item", async () => {
    const { target, content } = await setup("hello");
    click(target, "Numbered list");
    await new Promise(r => setTimeout(r, 0));
    expect(content.body.trim()).toBe("1. hello");
  });

  it("quote wraps the paragraph in a blockquote", async () => {
    const { target, content } = await setup("hello");
    click(target, "Quote");
    await new Promise(r => setTimeout(r, 0));
    expect(content.body.trim()).toBe("> hello");
  });

  it("code block wraps the paragraph in a fence", async () => {
    const { target, content } = await setup("hello");
    click(target, "Code block");
    await new Promise(r => setTimeout(r, 0));
    expect(content.body).toContain("```");
    expect(content.body).toContain("hello");
  });

  it("horizontal rule inserts a divider", async () => {
    const { target, content } = await setup("hello");
    click(target, "Horizontal rule");
    await new Promise(r => setTimeout(r, 0));
    expect(content.body).toMatch(/^---$/m);
  });

  it("task list produces a markdown checkbox", async () => {
    const { target, content } = await setup("hello");
    click(target, "Task list");
    await new Promise(r => setTimeout(r, 0));
    expect(content.body).toMatch(/- \[[ x]\] hello/);
  });

  it("heading dropdown applies a heading level", async () => {
    const { target, content } = await setup("hello");
    click(target, "Heading");
    await new Promise(r => setTimeout(r, 0));
    const items = [...target.querySelectorAll<HTMLButtonElement>(".fmt-dropdown-item")];
    const h2 = items.find(b => b.textContent?.trim() === "Heading 2");
    expect(h2, "Heading 2 option should exist").toBeTruthy();
    h2!.click();
    await new Promise(r => setTimeout(r, 0));
    expect(content.body.trim()).toBe("## hello");
  });

  it("read-only mode hides the toolbar", async () => {
    const target = document.createElement("div");
    document.body.appendChild(target);
    const content = { body: "hello" };
    const app = mount(RichEditor, { target, props: { content, editable: false } });
    cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
    await new Promise(r => setTimeout(r, 0));
    expect(target.querySelector(".format-bar")).toBeNull();
    expect(target.querySelector(".tiptap")).toBeTruthy();
  });
});

describe("RichEditor markdown round-trip", () => {
  it("preserves incoming markdown structure", async () => {
    const { content } = await setup("# Title\n\nsome **bold** text");
    await new Promise(r => setTimeout(r, 0));
    expect(content.body).toContain("# Title");
    expect(content.body).toContain("**bold**");
  });

  it("keeps a colour span through a load and re-serialise", async () => {
    // Colour has no markdown syntax; it survives only because html:true is set.
    const { target, content } = await setup('a <span style="color:#3182ce">blue</span> b');
    click(target, "Bullet list");
    await new Promise(r => setTimeout(r, 0));
    expect(content.body).toContain("#3182ce");
    expect(content.body).toContain("blue");
  });
});
