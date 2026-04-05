import { test, expect } from "playwright/test";
import { setupTauriMock } from "./mock";

test.beforeEach(async ({ page }) => {
  await setupTauriMock(page);
});

test("select button enters select mode", async ({ page }) => {
  await page.goto("/");
  await page.click(".select-btn");
  await expect(page.locator(".select-btn")).toHaveClass(/active/);
  // Checkboxes appear
  await expect(page.locator(".checkbox").first()).toBeVisible();
});

test("cancel exits select mode and clears selection", async ({ page }) => {
  await page.goto("/");
  await page.click(".select-btn");
  await page.locator(".note-card").first().click();
  // Exit select mode
  await page.click(".select-btn"); // now says "Cancel"
  await expect(page.locator(".checkbox")).toHaveCount(0);
  await expect(page.locator(".action-bar")).toHaveCount(0);
});

test("checking a note shows action bar", async ({ page }) => {
  await page.goto("/");
  await page.click(".select-btn");
  await page.locator(".note-card").first().click();
  await expect(page.locator(".action-bar")).toBeVisible();
  await expect(page.locator(".btn-send")).toBeVisible();
});

test("action bar shows correct count", async ({ page }) => {
  await page.goto("/");
  await page.click(".select-btn");
  await page.locator(".note-card").nth(0).click();
  await page.locator(".note-card").nth(1).click();
  await expect(page.locator(".sel-count")).toContainText("2 selected");
});

test("action bar hidden when nothing selected", async ({ page }) => {
  await page.goto("/");
  await page.click(".select-btn");
  // No notes checked yet
  await expect(page.locator(".action-bar")).toHaveCount(0);
});

test("deselecting a note removes it from count", async ({ page }) => {
  await page.goto("/");
  await page.click(".select-btn");
  await page.locator(".note-card").nth(0).click();
  await page.locator(".note-card").nth(1).click();
  await page.locator(".note-card").nth(0).click(); // deselect first
  await expect(page.locator(".sel-count")).toContainText("1 selected");
});

test("send selected opens transfer modal", async ({ page }) => {
  await setupTauriMock(page, {
    peers_scan: [],
    known_peers_list: [],
  });
  await page.goto("/");
  await page.click(".select-btn");
  await page.locator(".note-card").first().click();
  await page.click(".btn-send");
  await expect(page.locator(".modal")).toBeVisible();
});
