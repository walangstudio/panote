import { test, expect } from "playwright/test";
import { setupTauriMock } from "./mock";

test.beforeEach(async ({ page }) => {
  await setupTauriMock(page);
});

test("drawer is closed by default on all screen sizes", async ({ page }) => {
  await page.goto("/");
  const drawer = page.locator(".drawer");
  await expect(drawer).not.toHaveClass(/open/);
  // Closed means translated off-screen, not merely unstyled.
  const box = await drawer.boundingBox();
  expect(box!.x).toBeLessThan(0);
});

test("menu button opens the drawer", async ({ page }) => {
  await page.goto("/");
  await page.click(".menu-btn");
  const drawer = page.locator(".drawer");
  await expect(drawer).toHaveClass(/open/);
  // Wait out the 0.25s slide before asserting the on-screen position.
  await page.waitForTimeout(300);
  const box = await drawer.boundingBox();
  expect(box!.x).toBeGreaterThanOrEqual(0);
});

test("clicking the backdrop closes the drawer", async ({ page }) => {
  await page.goto("/");
  await page.click(".menu-btn");
  await expect(page.locator(".drawer")).toHaveClass(/open/);
  await page.click(".backdrop");
  await expect(page.locator(".drawer")).not.toHaveClass(/open/);
});

test("clicking a nav link closes the drawer", async ({ page }) => {
  await page.goto("/");
  await page.click(".menu-btn");
  await page.click(".nav-item");
  await expect(page.locator(".drawer")).not.toHaveClass(/open/);
});
