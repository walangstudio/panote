import { test, expect } from "playwright/test";
import { setupTauriMock, MOCK_NOTE_DETAIL } from "./mock";

test.beforeEach(async ({ page }) => {
  await setupTauriMock(page);
});

test("note editor loads existing note", async ({ page }) => {
  await page.goto("/note/note-1");
  await expect(page.locator(".title-input")).toHaveValue("Meeting notes");
});

test("existing tags shown as pills", async ({ page }) => {
  await page.goto("/note/note-1");
  await expect(page.locator(".tag").first()).toBeVisible();
  await expect(page.locator(".tag").first()).toContainText("work");
});

test("tag added on Enter key", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await page.fill(".tag-input", "mytag");
  await page.press(".tag-input", "Enter");
  await expect(page.locator(".tag")).toContainText("mytag");
  await expect(page.locator(".tag-input")).toHaveValue("");
});

test("tag added on comma key", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await page.fill(".tag-input", "mytag");
  await page.press(".tag-input", ",");
  await expect(page.locator(".tag")).toContainText("mytag");
});

// Regression: mobile blur fix — tag committed on blur (tapping away)
test("tag committed when input loses focus", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await page.fill(".tag-input", "blurtag");
  // Focus something else to trigger blur
  await page.click(".title-input");
  await expect(page.locator(".tag")).toContainText("blurtag");
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
  // Wait for navigation back to /
  await page.waitForURL("/");
  expect(savedTags).toContain("savetag");
});

test("duplicate tags not added", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await page.fill(".tag-input", "dup");
  await page.press(".tag-input", "Enter");
  await page.fill(".tag-input", "dup");
  await page.press(".tag-input", "Enter");
  const tagCount = await page.locator(".tag").count();
  expect(tagCount).toBe(1);
});

test("tag removed when × clicked", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.waitForSelector(".tag");
  const initialCount = await page.locator(".tag").count();
  await page.locator(".tag button").first().click();
  const newCount = await page.locator(".tag").count();
  expect(newCount).toBe(initialCount - 1);
});

test("··· menu opens on existing note", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.click(".menu-btn");
  await expect(page.locator(".menu-dropdown")).toBeVisible();
  await expect(page.locator(".menu-dropdown")).toContainText("Send note");
});

test("··· menu not shown on new note", async ({ page }) => {
  await page.goto("/note/new?kind=text");
  await expect(page.locator(".menu-btn")).not.toBeVisible();
});

test("Send note opens transfer modal", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.click(".menu-btn");
  await page.locator(".menu-dropdown button").click();
  await expect(page.locator(".modal")).toBeVisible();
});
