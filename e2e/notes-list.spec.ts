import { test, expect } from "playwright/test";
import { setupTauriMock, MOCK_NOTES } from "./mock";

test.beforeEach(async ({ page }) => {
  await setupTauriMock(page);
});

test("renders all notes from mock", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByText("Meeting notes")).toBeVisible();
  await expect(page.getByText("Shopping list")).toBeVisible();
  await expect(page.getByText("Draft")).toBeVisible();
});

test("search filters notes by title", async ({ page }) => {
  await page.goto("/");
  await page.fill(".search", "meeting");
  await expect(page.getByText("Meeting notes")).toBeVisible();
  await expect(page.getByText("Shopping list")).not.toBeVisible();
  await expect(page.getByText("Draft")).not.toBeVisible();
});

test("search filters notes by tag", async ({ page }) => {
  await page.goto("/");
  await page.fill(".search", "personal");
  await expect(page.getByText("Draft")).toBeVisible();
  await expect(page.getByText("Meeting notes")).not.toBeVisible();
});

test("search is case-insensitive", async ({ page }) => {
  await page.goto("/");
  await page.fill(".search", "SHOPPING");
  await expect(page.getByText("Shopping list")).toBeVisible();
});

test("empty state shows message when no notes match search", async ({ page }) => {
  await page.goto("/");
  await page.fill(".search", "zzz-no-match");
  await expect(page.getByText("No notes yet")).toBeVisible();
});

test("new note button shows kind picker", async ({ page }) => {
  await page.goto("/");
  await page.click(".new-btn");
  await expect(page.locator(".kind-picker")).toBeVisible();
  await expect(page.locator("#kind-select")).toBeVisible();
});

test("cancel hides kind picker", async ({ page }) => {
  await page.goto("/");
  await page.click(".new-btn");
  await page.click(".btn-cancel");
  await expect(page.locator(".kind-picker")).not.toBeVisible();
});

test("note card links to editor", async ({ page }) => {
  await page.goto("/");
  const href = await page.locator(".note-card").first().getAttribute("href");
  expect(href).toMatch(/\/note\//);
});
