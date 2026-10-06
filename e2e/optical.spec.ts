import { test, expect } from "playwright/test";
import { reject, setupTauriMock } from "./mock";
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

test("send by screen seals the note with the passphrase and plays a stream", async ({ page }) => {
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
  await page.getByRole("button", { name: "Send by screen" }).click();

  const start = page.getByRole("button", { name: "Start" });
  await page.getByLabel("Passphrase", { exact: true }).fill("123456789");
  await expect(start).toBeDisabled();
  await page.getByLabel("Passphrase", { exact: true }).fill("correct horse");
  await start.click();

  await expect(page.locator(".modal h2")).toHaveText("Show this to the camera");
  expect(packed).toEqual({ noteIds: ["note-1"], passphrase: "correct horse" });
  // The codes change frame to frame: two snapshots of the canvas differ.
  const canvas = page.locator("canvas.stream");
  const first = await canvas.evaluate((c: HTMLCanvasElement) => c.toDataURL());
  await expect
    .poll(() => canvas.evaluate((c: HTMLCanvasElement) => c.toDataURL()), { timeout: 5000 })
    .not.toBe(first);
});

test("receive by camera reassembles the stream and imports it with the passphrase", async ({ page }) => {
  const received: number[][] = [];
  await setupTauriMock(page, {
    optical_import: (args: unknown) => {
      const { payload, passphrase } = args as { payload: number[]; passphrase: string };
      received.push(payload);
      return passphrase === "correct horse" ? { inserted: 1, updated: 0 } : reject("wrong passphrase");
    },
  });
  await page.goto("/settings");
  await page.getByRole("button", { name: /Receive by camera/ }).click();

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
