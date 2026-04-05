import { test, expect } from "playwright/test";
import { setupTauriMock } from "./mock";

test.beforeEach(async ({ page }) => {
  await setupTauriMock(page);
});

test("sidebar is hidden by default on all screen sizes", async ({ page }) => {
  await page.goto("/");
  const sidebar = page.locator(".sidebar");
  // Should be translated off-screen
  const transform = await sidebar.evaluate((el) =>
    getComputedStyle(el).transform
  );
  // translateX(-100%) compiles to a matrix with a negative x translation
  expect(transform).toMatch(/matrix/);
  const box = await sidebar.boundingBox();
  // The sidebar should be off-screen (x < 0) when closed
  expect(box!.x).toBeLessThan(0);
});

test("hamburger button opens sidebar", async ({ page }) => {
  await page.goto("/");
  await page.click(".hamburger");
  const sidebar = page.locator(".sidebar");
  await expect(sidebar).toHaveClass(/open/);
  // Wait for 0.2s CSS transition to finish, then verify on-screen position
  await page.waitForTimeout(250);
  const box = await sidebar.boundingBox();
  expect(box!.x).toBeGreaterThanOrEqual(0);
});

test("clicking overlay closes sidebar", async ({ page }) => {
  await page.goto("/");
  await page.click(".hamburger");
  await expect(page.locator(".sidebar")).toHaveClass(/open/);
  await page.click(".overlay.visible");
  await expect(page.locator(".sidebar")).not.toHaveClass(/open/);
});

test("clicking a nav link closes sidebar", async ({ page }) => {
  await page.goto("/");
  await page.click(".hamburger");
  await page.click(".nav-item");
  await expect(page.locator(".sidebar")).not.toHaveClass(/open/);
});

test("sidebar shows app version", async ({ page }) => {
  await page.goto("/");
  await page.click(".hamburger");
  await expect(page.locator(".version")).toBeVisible();
  await expect(page.locator(".version")).toContainText("v0.2.0");
});
