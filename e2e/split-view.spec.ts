import { test, expect } from "playwright/test";
import { setupTauriMock } from "./mock";

test.beforeEach(async ({ page }) => {
  await setupTauriMock(page);
});

test("wide windows get the list + detail split", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  await expect(page.locator(".split")).toBeVisible();
  await expect(page.locator(".list-pane")).toBeVisible();
  // The list is present, and the detail pane prompts for a selection.
  await expect(page.getByText("Meeting notes")).toBeVisible();
  await expect(page.getByText("Select a note")).toBeVisible();
});

test("narrow windows keep the single-column touch layout", async ({ page }) => {
  await page.setViewportSize({ width: 600, height: 800 });
  await page.goto("/");
  await expect(page.locator(".split")).toHaveCount(0);
  await expect(page.getByText("Meeting notes")).toBeVisible();
});

test("selecting a note keeps the list visible on wide windows", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  await page.locator(".note-card").first().click();
  // The whole point of the split: the list survives the navigation.
  await expect(page.locator(".list-pane")).toBeVisible();
  await expect(page.locator(".title-input")).toBeVisible();
});
