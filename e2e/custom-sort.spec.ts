import { test, expect, type Page } from "playwright/test";
import { setupTauriMock, MOCK_NOTES } from "./mock";

// A backend that keeps sort_order the way notes_reorder writes it, so the list
// after a drop and after a reload is what the database would hand back.
async function withOrder(page: Page) {
  const order = new Map(MOCK_NOTES.map((n, i) => [n.id, i]));
  const calls: string[][] = [];
  await setupTauriMock(page, {
    note_list: () => MOCK_NOTES.map(n => ({ ...n, sort_order: order.get(n.id) })),
    notes_reorder: ({ ids }: { ids: string[] }) => {
      calls.push(ids);
      ids.forEach((id, i) => order.set(id, i));
      return null;
    },
  });
  return calls;
}

async function chooseCustom(page: Page) {
  await page.getByRole("button", { name: "Sort notes" }).first().click();
  await page.locator(".sort-option", { hasText: "Custom" }).first().click();
  await page.locator(".sort-backdrop").first().click();
}

const titles = (page: Page) => page.locator(".note-list .note-card strong").allTextContents();
const grip = (page: Page, title: string) =>
  page.locator(".note-card", { hasText: title }).locator(".drag-grip").first();
const center = async (page: Page, title: string) => {
  const box = (await page.locator(".note-card", { hasText: title }).first().boundingBox())!;
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
};

test("a mouse drag on the grip moves a note and the order survives a reload", async ({ page }) => {
  const calls = await withOrder(page);
  await page.goto("/");
  await chooseCustom(page);
  await expect.poll(() => titles(page)).toEqual(["Meeting notes", "Shopping list", "Draft"]);

  const from = (await grip(page, "Draft").boundingBox())!;
  const to = await center(page, "Meeting notes");
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 12 });
  await expect.poll(() => titles(page)).toEqual(["Draft", "Meeting notes", "Shopping list"]);
  await page.mouse.up();

  await expect.poll(() => calls).toEqual([["note-3", "note-1", "note-2"]]);
  await expect.poll(() => titles(page)).toEqual(["Draft", "Meeting notes", "Shopping list"]);

  await page.reload();
  await expect.poll(() => titles(page)).toEqual(["Draft", "Meeting notes", "Shopping list"]);
  await expect(grip(page, "Draft")).toBeVisible();
});

test("other sorts hide the grip", async ({ page }) => {
  await withOrder(page);
  await page.goto("/");
  await expect(page.locator(".note-list .note-card").first()).toBeVisible();
  await expect(page.locator(".drag-grip")).toHaveCount(0);
});

test("the grip moves a note with the keyboard and announces it", async ({ page }) => {
  const calls = await withOrder(page);
  await page.goto("/");
  await chooseCustom(page);

  await grip(page, "Shopping list").focus();
  await page.keyboard.press("ArrowUp");

  await expect.poll(() => titles(page)).toEqual(["Shopping list", "Meeting notes", "Draft"]);
  await expect.poll(() => calls).toEqual([["note-2", "note-1", "note-3"]]);
  await expect(grip(page, "Shopping list")).toBeFocused();
  await expect(page.locator("[aria-live]", { hasText: "Shopping list moved to position 1 of 3" })).toHaveCount(1);
});

test.describe("touch", () => {
  test.use({ hasTouch: true });

  // Real touch input through the browser's pipeline, so touch-action and the
  // pointer events Chromium derives from it are exercised the way a phone does.
  test("a touch drag on the grip moves a note", async ({ page }) => {
    const calls = await withOrder(page);
    await page.goto("/");
    await chooseCustom(page);
    await expect.poll(() => titles(page)).toEqual(["Meeting notes", "Shopping list", "Draft"]);

    const cdp = await page.context().newCDPSession(page);
    const from = (await grip(page, "Draft").boundingBox())!;
    const to = await center(page, "Meeting notes");
    const x = from.x + from.width / 2;
    const y0 = from.y + from.height / 2;
    await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y: y0 }] });
    for (let i = 1; i <= 12; i++) {
      const y = y0 + ((to.y - y0) * i) / 12;
      await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x, y }] });
    }
    await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });

    await expect.poll(() => calls).toEqual([["note-3", "note-1", "note-2"]]);
    await expect.poll(() => titles(page)).toEqual(["Draft", "Meeting notes", "Shopping list"]);
  });

  test("a swipe off the grip still scrolls the list", async ({ page }) => {
    const many = Array.from({ length: 30 }, (_, i) => ({ ...MOCK_NOTES[0], id: `n${i}`, title: `Note ${i}`, sort_order: i }));
    const calls: string[][] = [];
    await setupTauriMock(page, {
      note_list: many,
      note_count: many.length,
      notes_reorder: ({ ids }: { ids: string[] }) => { calls.push(ids); return null; },
    });
    await page.goto("/");
    await chooseCustom(page);
    const scrolled = () => page.evaluate(() =>
      [document.scrollingElement!, ...document.querySelectorAll("*")].reduce((sum, el) => sum + el.scrollTop, 0));

    const cdp = await page.context().newCDPSession(page);
    const box = (await page.locator(".note-card", { hasText: "Note 5" }).locator(".note-info").boundingBox())!;
    const x = box.x + box.width / 2;
    const y0 = box.y + box.height / 2;
    await cdp.send("Input.dispatchTouchEvent", { type: "touchStart", touchPoints: [{ x, y: y0 }] });
    for (let i = 1; i <= 12; i++) {
      await cdp.send("Input.dispatchTouchEvent", { type: "touchMove", touchPoints: [{ x, y: y0 - i * 20 }] });
    }
    await cdp.send("Input.dispatchTouchEvent", { type: "touchEnd", touchPoints: [] });

    await expect.poll(scrolled).toBeGreaterThan(0);
    expect(calls).toEqual([]);
  });
});
