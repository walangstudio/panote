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
  // "No notes yet" is the empty-library wording; a search that matches nothing
  // says so specifically, because the two mean different things to the reader.
  await expect(page.getByText("No notes match your search.")).toBeVisible();
});

// The kind <select> is gone. Desktop composes from the pane header into a modal
// listing the kinds; touch fans the same choices out of a FAB. Both are covered
// because they are separate markup, and the default viewport only renders one.
test("new note button shows the kind options", async ({ page }) => {
  await page.goto("/");
  await page.click(".compose-btn");
  await expect(page.locator(".modal")).toBeVisible();
  await expect(page.locator(".kind-row", { hasText: "Document" })).toBeVisible();
});

test("cancel hides the kind options", async ({ page }) => {
  await page.goto("/");
  await page.click(".compose-btn");
  await expect(page.locator(".modal")).toBeVisible();
  await page.click(".btn-cancel");
  await expect(page.locator(".modal")).toHaveCount(0);
});

test("touch layout offers the same kinds from the FAB", async ({ page }) => {
  await page.setViewportSize({ width: 500, height: 900 });
  await page.goto("/");
  await page.click(".fab");
  await expect(page.locator(".fab-options")).toBeVisible();
  await expect(page.locator(".fab-option", { hasText: "Document" })).toBeVisible();
});

// The card is a div with a click handler now, not an anchor, so there is no href
// to read - assert the navigation it performs instead.
test("note card opens the editor", async ({ page }) => {
  await page.goto("/");
  await page.locator(".note-card").first().click();
  await expect(page).toHaveURL(/\/note\/note-/);
});
