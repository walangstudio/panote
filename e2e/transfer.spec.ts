import { test, expect } from "playwright/test";
import { setupTauriMock } from "./mock";

test.beforeEach(async ({ page }) => {
  await setupTauriMock(page);
});

// ---- TransferModal ----

test("transfer modal shows peer list step by default", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.click(".menu-btn");
  await page.locator(".menu-dropdown button").click();
  await expect(page.locator(".modal h2")).toContainText("Send");
  await expect(page.locator("text=Nearby devices")).toBeVisible();
});

test("transfer modal shows no devices when peers_scan returns empty", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.click(".menu-btn");
  await page.locator(".menu-dropdown button").click();
  await expect(page.locator("text=No devices found")).toBeVisible();
});

test("transfer modal shows discovered peers", async ({ page }) => {
  await setupTauriMock(page, {
    peers_scan: [{ id: "peer-1", name: "Alice Phone", address: "192.168.1.5", port: 47291, via: "lan" }],
    known_peers_list: [],
  });
  await page.goto("/note/note-1");
  await page.click(".menu-btn");
  await page.locator(".menu-dropdown button").click();
  await expect(page.locator("text=Alice Phone")).toBeVisible();
});

test("selecting peer enables Next button", async ({ page }) => {
  await setupTauriMock(page, {
    peers_scan: [{ id: "peer-1", name: "Alice Phone", address: "192.168.1.5", port: 47291, via: "lan" }],
    known_peers_list: [],
  });
  await page.goto("/note/note-1");
  await page.click(".menu-btn");
  await page.locator(".menu-dropdown button").click();
  const nextBtn = page.locator(".btn-primary", { hasText: "Next" });
  await expect(nextBtn).toBeDisabled();
  await page.locator(".peer-item").click();
  await expect(nextBtn).not.toBeDisabled();
});

test("pairing code step shows generated code", async ({ page }) => {
  await setupTauriMock(page, {
    peers_scan: [{ id: "peer-1", name: "Alice Phone", address: "192.168.1.5", port: 47291, via: "lan" }],
    known_peers_list: [],
    generate_pairing_code: "XYZ789",
  });
  await page.goto("/note/note-1");
  await page.click(".menu-btn");
  await page.locator(".menu-dropdown button").click();
  await page.locator(".peer-item").click();
  await page.locator(".btn-primary", { hasText: "Next" }).click();
  await expect(page.locator(".code-display")).toContainText("XYZ");
  await expect(page.locator(".code-display")).toContainText("789");
});

test("closing modal via × button works", async ({ page }) => {
  await page.goto("/note/note-1");
  await page.click(".menu-btn");
  await page.locator(".menu-dropdown button").click();
  await expect(page.locator(".modal")).toBeVisible();
  await page.locator(".modal .close").click();
  await expect(page.locator(".modal")).not.toBeVisible();
});

// ---- IncomingTransferToast ----

test("incoming transfer toast appears when there are pending transfers", async ({ page }) => {
  await setupTauriMock(page, {
    pending_transfers_list: [
      { transfer_id: "t-abc", from_peer: "bob-phone.local", received_at: 1700010000 },
    ],
  });
  await page.goto("/");
  await expect(page.locator(".toast")).toBeVisible();
  await expect(page.locator(".toast")).toContainText("bob-phone.local");
});

test("no toast when no pending transfers", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".toast")).toHaveCount(0);
});

test("toast shows error on empty code submit", async ({ page }) => {
  await setupTauriMock(page, {
    pending_transfers_list: [
      { transfer_id: "t-abc", from_peer: "bob-phone.local", received_at: 1700010000 },
    ],
  });
  await page.goto("/");
  await page.locator(".btn-accept").click();
  await expect(page.locator(".err")).toContainText("Enter the code");
});

test("toast accept with code calls noteReceiveAccept", async ({ page }) => {
  let acceptedId = "";
  let acceptedCode = "";
  await setupTauriMock(page, {
    pending_transfers_list: [
      { transfer_id: "t-abc", from_peer: "bob-phone.local", received_at: 1700010000 },
    ],
    note_receive_accept: (args: any) => {
      acceptedId = args?.transferId;
      acceptedCode = args?.passphrase;
      return "imported-id";
    },
    note_list: [],
  });
  await page.goto("/");
  await page.fill(".code-input", "ABC123");
  await page.locator(".btn-accept").click();
  await page.waitForTimeout(500);
  expect(acceptedId).toBe("t-abc");
  expect(acceptedCode).toBe("ABC123");
});
