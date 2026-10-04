import { test, expect, type Page } from "playwright/test";
import { setupTauriMock, reject, MOCK_NOTES, MOCK_NOTE_DETAIL } from "./mock";

// A protected note's title is sealed under its password, so the backend lists it
// under a placeholder until it is unlocked this session. The list has to re-read
// on unlock, or it keeps showing the placeholder beside an open note.
const LOCKED_TITLE = "Locked note";
const REAL_TITLE = "Swiss bank account";

async function mockProtectedNote(page: Page) {
  let unlocked = false;
  await setupTauriMock(page, {
    note_list: () => [
      {
        id: "secret",
        kind: "document",
        title: unlocked ? REAL_TITLE : LOCKED_TITLE,
        tags: [],
        created_at: 1700000000,
        updated_at: 1700020000,
        has_note_password: true,
        show_preview: true,
      },
      ...MOCK_NOTES,
    ],
    note_count: MOCK_NOTES.length + 1,
    note_get: () =>
      unlocked
        ? { ...MOCK_NOTE_DETAIL, id: "secret", kind: "document", title: REAL_TITLE, has_note_password: true }
        : reject("locked"),
    note_unlock: () => {
      unlocked = true;
      return null;
    },
  });
}

const card = (page: Page) => page.locator(".note-card", { has: page.locator(".lock") });

test("a locked note is listed under the placeholder, never its title", async ({ page }) => {
  await mockProtectedNote(page);
  await page.goto("/");
  await expect(card(page).locator(".title-row strong")).toHaveText(LOCKED_TITLE);
  await expect(page.getByText(REAL_TITLE)).toHaveCount(0);
});

test("unlocking the note shows its real title in the list", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await mockProtectedNote(page);
  await page.goto("/");
  await card(page).click();
  await expect(page.getByText("This note is locked")).toBeVisible();

  await page.fill(".lock-gate-input", "hunter2");
  await page.click(".unlock-btn");

  await expect(card(page).locator(".title-row strong")).toHaveText(REAL_TITLE);
  await expect(page.locator(".title-input")).toHaveValue(REAL_TITLE);
});
