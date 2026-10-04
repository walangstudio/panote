import { test, expect, type Page } from "playwright/test";
import { setupTauriMock, MOCK_NOTES } from "./mock";

// The native context menu needs the Tauri runtime, which the mock does not
// provide, so every menu here is the in-app fallback popover.

const many = (n: number) =>
  Array.from({ length: n }, (_, i) => ({ ...MOCK_NOTES[0], id: `n-${i}`, title: `Note ${i}`, tags: [] }));

const WORK = { id: "f-work", parent_id: null, name: "Work", note_count: 0 };

const card = (page: Page, title: string) => page.locator(".note-card", { hasText: title }).first();

async function insideViewport(page: Page, box: { x: number; y: number; width: number; height: number }) {
  const vp = page.viewportSize()!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.y).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(vp.width);
  expect(box.y + box.height).toBeLessThanOrEqual(vp.height);
}

test("a card menu near the bottom opens upward, fully on screen", async ({ page }) => {
  await setupTauriMock(page, { note_list: many(12), note_count: 12 });
  await page.setViewportSize({ width: 1280, height: 500 });
  await page.goto("/");
  await expect(card(page, "Note 0")).toBeVisible();

  // The lowest card that is entirely in view.
  const cards = page.locator(".note-list .note-card");
  let low = cards.first();
  for (let i = 0; i < (await cards.count()); i++) {
    const b = (await cards.nth(i).boundingBox())!;
    if (b.y + b.height <= 500) low = cards.nth(i);
  }
  const kebab = low.getByRole("button", { name: "More options" });
  const kebabBox = (await kebab.boundingBox())!;
  await kebab.click();

  const menu = page.locator(".card-popover");
  await expect(menu).toBeVisible();
  const box = (await menu.boundingBox())!;
  await insideViewport(page, box);
  expect(box.y + box.height).toBeLessThanOrEqual(kebabBox.y + 1);
  await expect(menu.locator(".popover-item", { hasText: "Delete" })).toBeInViewport({ ratio: 1 });
});

test("right-click opens the menu at the cursor", async ({ page }) => {
  await setupTauriMock(page);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  const target = card(page, "Meeting notes");
  const box = (await target.boundingBox())!;
  await target.click({ button: "right", position: { x: 40, y: 20 } });

  const menu = page.locator(".card-popover");
  await expect(menu).toBeVisible();
  const at = (await menu.boundingBox())!;
  expect(Math.abs(at.x - (box.x + 40))).toBeLessThanOrEqual(1);
  expect(Math.abs(at.y - (box.y + 20))).toBeLessThanOrEqual(1);
  await expect(menu).toContainText("Move to");

  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
});

for (const [name, size, scroller] of [
  ["phone", { width: 390, height: 844 }, ".app-content"],
  ["desktop", { width: 1280, height: 800 }, ".list-pane"],
] as const) {
  test(`the list header stays put while a long list scrolls (${name})`, async ({ page }) => {
    await setupTauriMock(page, { note_list: many(40), note_count: 40 });
    await page.setViewportSize(size);
    await page.goto("/");
    await expect(card(page, "Note 0")).toBeVisible();

    await page.locator(scroller).evaluate(el => el.scrollTo(0, el.scrollHeight));
    await expect(page.locator(".note-list .note-card").last()).toBeInViewport();
    const search = page.locator(".search");
    await expect(search).toBeInViewport({ ratio: 1 });
    // Not merely in view: on top, so it can be clicked.
    await search.click();
    await expect(search).toBeFocused();
    await expect(page.getByRole("button", { name: "Sort notes" })).toBeInViewport({ ratio: 1 });
  });
}

test("a note created inside a folder is filed there", async ({ page }) => {
  let input: Record<string, unknown> = {};
  await setupTauriMock(page, {
    folder_list: [WORK],
    note_create: (args: { input: Record<string, unknown> }) => {
      input = args.input;
      return { ...MOCK_NOTES[0], id: "new-id", title: "Filed", folder_id: input.folder_id };
    },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  await card(page, "Work").click();
  await expect(page.locator(".crumb.current")).toHaveText("Work");

  await page.getByRole("button", { name: "New note", exact: true }).click();
  await page.getByRole("dialog", { name: "New note" }).getByRole("button", { name: /Checklist/ }).click();
  await page.waitForURL(/folder=f-work/);
  await page.fill(".title-input", "Filed");
  await page.click(".save-btn");
  await page.waitForURL(/\/note\/new-id/);
  expect(input.folder_id).toBe("f-work");
});

test("the folder menu offers a new note inside it", async ({ page }) => {
  await setupTauriMock(page, { folder_list: [WORK] });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  await page.getByRole("button", { name: "Actions for Work", exact: true }).click();
  await page.locator(".popover-item", { hasText: "New note inside" }).click();
  await expect(page.getByRole("dialog", { name: "New note" })).toBeVisible();
  await expect(page.locator(".crumb.current")).toHaveText("Work");
});

test.describe("keyboard shortcuts", () => {
  test.beforeEach(async ({ page }) => {
    await setupTauriMock(page);
    await page.setViewportSize({ width: 1280, height: 800 });
  });

  test("Ctrl+N opens the new-note chooser and Esc closes it", async ({ page }) => {
    await page.goto("/");
    await expect(card(page, "Meeting notes")).toBeVisible();
    await page.keyboard.press("Control+n");
    const chooser = page.getByRole("dialog", { name: "New note" });
    await expect(chooser).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(chooser).toHaveCount(0);
  });

  test("Ctrl+Shift+N opens the new-folder dialog", async ({ page }) => {
    await page.goto("/");
    await expect(card(page, "Meeting notes")).toBeVisible();
    await page.keyboard.press("Control+Shift+N");
    await expect(page.locator(".modal")).toContainText("New folder");
  });

  test("Ctrl+F focuses search", async ({ page }) => {
    await page.goto("/");
    await expect(card(page, "Meeting notes")).toBeVisible();
    await page.keyboard.press("Control+f");
    await expect(page.locator(".search")).toBeFocused();
  });

  test("Delete asks to move the open note to Trash", async ({ page }) => {
    await page.goto("/note/note-1");
    await expect(page.locator(".title-input")).toHaveValue("Meeting notes");
    await page.locator(".wordmark").click();
    await page.keyboard.press("Delete");
    await expect(page.locator(".modal")).toContainText("Move to Trash?");
    await expect(page.locator(".modal")).toContainText("This note will be moved");
  });

  test("Delete asks to move the selected notes to Trash", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Select notes" }).click();
    await card(page, "Meeting notes").click();
    await card(page, "Draft").click();
    await page.keyboard.press("Delete");
    await expect(page.locator(".modal")).toContainText("2 notes will be moved to Trash");
  });

  test("shortcuts leave typing alone", async ({ page }) => {
    await page.goto("/");
    await page.locator(".search").click();
    await page.keyboard.press("Control+n");
    await page.keyboard.press("Delete");
    await expect(page.locator(".modal")).toHaveCount(0);
  });
});
