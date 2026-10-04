import { test, expect } from "playwright/test";
import { setupTauriMock } from "./mock";

// A fresh install has nothing stored anywhere, so the theme has to come from
// the OS. get_theme answers null the way an empty settings table does.
test.beforeEach(async ({ page }) => {
  await setupTauriMock(page, { get_theme: null });
});

test("a fresh install follows a dark OS", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "candy-dark");
});

test("switches live when the OS setting changes", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await page.goto("/");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "candy-light");
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "candy-dark");
});

test("an explicit theme picked in the drawer ignores the OS", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/");
  await page.click(".menu-btn");
  await page.locator(".drawer select").selectOption("candy-light");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "candy-light");
  await page.emulateMedia({ colorScheme: "light" });
  await page.emulateMedia({ colorScheme: "dark" });
  await expect(page.locator("html")).toHaveAttribute("data-theme", "candy-light");
});

test("settings can hand the theme back to the OS", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "dark" });
  await page.goto("/settings");
  const pick = page.locator("#theme-select");
  await pick.selectOption("candy-light");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "candy-light");
  await pick.selectOption("system");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "candy-dark");
});
