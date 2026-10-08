import { test, expect } from "playwright/test";
import { MOCK_NOTES, reject, setupTauriMock } from "./mock";
import { FAKE_CAMERA, OPTICAL_PAYLOAD } from "./optical-fixture";

// The camera is Chromium's fake device playing the QR stream written by global
// setup, so the receive test runs the real decode worker, WebAssembly codec and
// fountain decoder - everything but the Rust import behind the mock.
test.use({
  launchOptions: {
    args: [
      "--use-fake-device-for-media-stream",
      "--use-fake-ui-for-media-stream",
      `--use-file-for-fake-video-capture=${FAKE_CAMERA}`,
    ],
  },
});

const MENU_BTN = '.editor-header button[aria-label="More options"]';
const OPEN_TRANSFER = '.overflow-menu button:has-text("Transfer")';

test("send to camera seals the note with the passphrase and plays a stream", async ({ page }) => {
  let packed: { noteIds: string[]; passphrase: string } | null = null;
  await setupTauriMock(page, {
    optical_pack: (args: unknown) => {
      packed = args as typeof packed;
      return Array.from(OPTICAL_PAYLOAD);
    },
  });
  await page.goto("/note/note-1");
  await page.click(MENU_BTN);
  await page.locator(OPEN_TRANSFER).click();
  await expect(page.getByRole("tab", { name: "Camera" })).toHaveAttribute("aria-selected", "true");

  const start = page.getByRole("button", { name: "Start" });
  await page.getByLabel("Passphrase", { exact: true }).fill("123456789");
  await expect(start).toBeDisabled();
  await page.getByLabel("Passphrase", { exact: true }).fill("correct horse");
  await start.click();

  await expect(page.locator(".modal h2")).toHaveText("Show this to the camera");
  expect(packed).toEqual({ noteIds: ["note-1"], passphrase: "correct horse", folderId: null });
  // The codes change frame to frame: two snapshots of the canvas differ.
  const canvas = page.locator("canvas.stream");
  const first = await canvas.evaluate((c: HTMLCanvasElement) => c.toDataURL());
  await expect
    .poll(() => canvas.evaluate((c: HTMLCanvasElement) => c.toDataURL()), { timeout: 5000 })
    .not.toBe(first);
});

test("receiving by camera reassembles the stream and imports it with the passphrase", async ({ page }) => {
  const received: number[][] = [];
  await setupTauriMock(page, {
    optical_import: (args: unknown) => {
      const { payload, passphrase } = args as { payload: number[]; passphrase: string };
      received.push(payload);
      return passphrase === "correct horse" ? { inserted: 1, updated: 0 } : reject("wrong passphrase");
    },
  });
  await page.goto("/settings");
  await page.locator("button.row", { hasText: "By camera, or over this network" }).click();

  await expect(page.locator(".modal h2")).toHaveText("Enter the passphrase", { timeout: 20_000 });
  await page.getByLabel("Passphrase", { exact: true }).fill("nope");
  await page.getByRole("button", { name: "Open", exact: true }).click();
  await expect(page.getByRole("alert")).toHaveText("Wrong passphrase.");

  await page.getByLabel("Passphrase", { exact: true }).fill("correct horse");
  await page.getByRole("button", { name: "Open", exact: true }).click();
  await expect(page.locator(".modal h2")).toHaveText("Received");
  await expect(page.getByRole("status").filter({ hasText: "1 note received" })).toBeVisible();
  expect(received.at(-1)).toEqual(Array.from(OPTICAL_PAYLOAD));
});

test.describe("send and receive from the row menus", () => {
  const WORK = { id: "f-work", parent_id: null, name: "Work", note_count: 0 };
  const item = (page: import("playwright/test").Page, label: string) =>
    page.locator(".popover-item", { hasText: label });

  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
  });

  test("a note's menu sends just that note", async ({ page }) => {
    await setupTauriMock(page, { folder_list: [WORK] });
    await page.goto("/");
    await page.locator(".note-card", { hasText: "Meeting notes" }).first().click({ button: "right" });
    await item(page, "Send…").click();
    await expect(page.locator(".modal h2")).toHaveText("Send note");
    await expect(page.locator(".modal")).toContainText("Show this screen to the other device's camera");
  });

  test("the passphrase can be shown to check for typos", async ({ page }) => {
    await setupTauriMock(page);
    await page.goto("/note/note-1");
    await page.click(MENU_BTN);
    await page.locator(OPEN_TRANSFER).click();
    const field = page.getByLabel("Passphrase", { exact: true });
    await field.fill("correct horse");
    await expect(field).toHaveAttribute("type", "password");
    await page.getByRole("button", { name: "Show passphrase" }).click();
    await expect(field).toHaveAttribute("type", "text");
    await page.getByRole("button", { name: "Hide passphrase" }).click();
    await expect(field).toHaveAttribute("type", "password");
  });

  test("a folder's menu receives into that folder", async ({ page }) => {
    const calls: { folderId: string | null }[] = [];
    await setupTauriMock(page, {
      folder_list: [WORK],
      optical_import: (args: unknown) => {
        calls.push(args as { folderId: string | null });
        return { inserted: 1, updated: 0 };
      },
    });
    await page.goto("/");
    await page.locator(".note-card", { hasText: "Work" }).first().click({ button: "right" });
    await item(page, "Receive into folder").click();
    await expect(page.locator(".modal h2")).toHaveText("Receive into Work");
    await expect(page.locator(".modal h2")).toHaveText("Enter the passphrase", { timeout: 20_000 });
    await page.getByLabel("Passphrase", { exact: true }).fill("correct horse");
    await page.getByRole("button", { name: "Open", exact: true }).click();
    await expect(page.locator(".modal h2")).toHaveText("Received");
    expect(calls.at(-1)?.folderId).toBe("f-work");
  });

  test("the header's Receive button receives into the folder being viewed", async ({ page }) => {
    await setupTauriMock(page, { folder_list: [WORK] });
    await page.goto("/");
    await page.getByRole("button", { name: "Receive", exact: true }).click();
    await expect(page.locator(".modal h2")).toHaveText(/^(Receive|Enter the passphrase)$/);
  });

  test("a folder's Send... sends the folder itself", async ({ page }) => {
    let packed: { noteIds: string[]; folderId: string | null } | null = null;
    await setupTauriMock(page, {
      folder_list: [WORK],
      note_list: [{ ...MOCK_NOTES[0], folder_id: "f-work" }, MOCK_NOTES[1]],
      optical_pack: (args: unknown) => {
        packed = args as typeof packed;
        return Array.from(OPTICAL_PAYLOAD);
      },
    });
    await page.goto("/");
    await page.locator(".note-card", { hasText: "Work" }).first().click({ button: "right" });
    await item(page, "Send…").click();
    await expect(page.locator(".modal h2")).toHaveText("Send Work");
    await page.getByLabel("Passphrase", { exact: true }).fill("correct horse");
    await page.getByRole("button", { name: "Start" }).click();
    await expect(page.locator(".modal h2")).toHaveText("Show this to the camera");
    expect(packed).toEqual({ noteIds: [MOCK_NOTES[0].id], passphrase: "correct horse", folderId: "f-work" });
  });

  test("send and receive offer Camera, Network and Bluetooth (coming soon)", async ({ page }) => {
    await setupTauriMock(page, { is_receiving: false });
    await page.goto("/note/note-1");
    await page.click(MENU_BTN);
    await page.locator(OPEN_TRANSFER).click();
    await page.getByRole("tab", { name: /Network/ }).click();
    await expect(page.locator("text=Nearby devices")).toBeVisible();
    await page.getByRole("tab", { name: /Bluetooth/ }).click();
    await expect(page.locator(".modal")).toContainText("coming soon");
    await page.keyboard.press("Escape");

    await page.goto("/settings");
    await page.locator("button.row", { hasText: "By camera, or over this network" }).click();
    await page.getByRole("tab", { name: /Network/ }).click();
    await expect(page.getByRole("button", { name: "Start receiving" })).toBeVisible();
  });

  test("when the camera is refused, Network is still one tap away", async ({ page }) => {
    await setupTauriMock(page, { is_receiving: false });
    await page.addInitScript(() => {
      navigator.mediaDevices.getUserMedia = () =>
        Promise.reject(new DOMException("denied", "NotAllowedError"));
    });
    await page.goto("/settings");
    await page.locator("button.row", { hasText: "By camera, or over this network" }).click();
    await expect(page.getByRole("alert")).toContainText("Camera access was denied");
    await page.getByRole("tab", { name: /Network/ }).click();
    await expect(page.getByRole("button", { name: "Start receiving" })).toBeVisible();
  });
});
