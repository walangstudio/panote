import { test, expect } from "playwright/test";
import { setupTauriMock } from "./mock";

// At the default 1280px viewport the desktop split renders the note list beside
// the editor, and the list has `.tag` pills of its own. Every tag assertion here
// has to be scoped to the editor or it silently counts the list's tags too.
const TAG = ".tags-row .tag-chip";
const TAG_REMOVE = ".tags-row .tag-remove";

/// The overflow trigger carries no stable class of its own (`.bare-icon` is
/// shared), so go through the accessible name - scoped to the editor header,
/// because every card in the list pane beside it has a "More options" too.
const MENU_BTN = '.editor-header button[aria-label="More options"]';

test.beforeEach(async ({ page }) => {
  await setupTauriMock(page);
});

test("note editor loads existing note", async ({ page }) => {
  await page.goto("/note/note-1");
  await expect(page.locator(".title-input")).toHaveValue("Meeting notes");
});

test("existing tags shown as pills", async ({ page }) => {
  await page.goto("/note/note-1");
  await expect(page.locator(TAG).first()).toBeVisible();
  await expect(page.locator(TAG).first()).toContainText("work");
});

test("tag added on Enter key", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await page.fill(".tag-input", "mytag");
  await page.press(".tag-input", "Enter");
  await expect(page.locator(TAG)).toContainText("mytag");
  await expect(page.locator(".tag-input")).toHaveValue("");
});

test("tag added on comma key", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await page.fill(".tag-input", "mytag");
  await page.press(".tag-input", ",");
  await expect(page.locator(TAG)).toContainText("mytag");
});

// Regression: mobile blur fix — tag committed on blur (tapping away)
test("tag committed when input loses focus", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await page.fill(".tag-input", "blurtag");
  // Focus something else to trigger blur
  await page.click(".title-input");
  await expect(page.locator(TAG)).toContainText("blurtag");
});

// Regression: mobile save fix — pending tag flushed on Save
test("pending tag flushed when Save clicked without blurring", async ({ page }) => {
  let savedTags: string[] = [];
  await setupTauriMock(page, {
    note_create: (args: any) => {
      savedTags = args?.input?.tags ?? [];
      return { id: "new-id", kind: "text", title: "", tags: savedTags, created_at: 0, updated_at: 0, has_note_password: false };
    },
    note_list: [],
  });
  await page.goto("/note/new?kind=text");
  await page.fill(".title-input", "Test");
  // Type tag but do NOT blur — click Save directly
  await page.fill(".tag-input", "savetag");
  await page.click(".save-btn");
  // Saving a new note now stays in the editor on the note that was just created,
  // rather than returning to the list. Waiting on that is only the sync point -
  // the assertion is that the untouched tag input still reached the save.
  await page.waitForURL(/\/note\/new-id/);
  expect(savedTags).toContain("savetag");
});

test("duplicate tags not added", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await page.fill(".tag-input", "dup");
  await page.press(".tag-input", "Enter");
  await page.fill(".tag-input", "dup");
  await page.press(".tag-input", "Enter");
  const tagCount = await page.locator(TAG).count();
  expect(tagCount).toBe(1);
});

test("tag removed when × clicked", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.waitForSelector(TAG);
  const initialCount = await page.locator(TAG).count();
  await page.locator(TAG_REMOVE).first().click();
  const newCount = await page.locator(TAG).count();
  expect(newCount).toBe(initialCount - 1);
});

test("··· menu opens on existing note", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.click(MENU_BTN);
  await expect(page.locator(".overflow-menu")).toBeVisible();
  await expect(page.locator(".overflow-menu")).toContainText("Transfer");
});

test("··· menu not shown on new note", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await expect(page.locator(MENU_BTN)).not.toBeVisible();
});

test("Send note opens transfer modal", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.click(MENU_BTN);
  // The menu holds Background, Transfer, password and delete items - name the
  // one we mean rather than taking whichever comes first.
  await page.locator(".overflow-menu button", { hasText: "Transfer" }).click();
  await expect(page.locator(".modal")).toBeVisible();
});
