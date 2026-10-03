import { test, expect, type Page } from "playwright/test";
import { setupTauriMock, MOCK_NOTES } from "./mock";

// A stateful backend: delete moves a note to trash, restore moves it back, and
// delete-forever drops it, so the spec sees what a user would across the round trip.
async function withTrash(page: Page) {
  let live = MOCK_NOTES.map(n => ({ ...n }));
  let binned: Array<(typeof live)[number] & { deleted_at: number }> = [];
  await setupTauriMock(page, {
    note_list: () => live,
    note_count: () => live.length,
    notes_delete: ({ ids }: { ids: string[] }) => {
      const now = Math.floor(Date.now() / 1000);
      binned = [...live.filter(x => ids.includes(x.id)).map(n => ({ ...n, deleted_at: now })), ...binned];
      live = live.filter(x => !ids.includes(x.id));
      return null;
    },
    trash_list: () => binned,
    trash_restore: ({ ids }: { ids: string[] }) => {
      live = [...live, ...binned.filter(x => ids.includes(x.id))];
      binned = binned.filter(x => !ids.includes(x.id));
      return null;
    },
    trash_delete: ({ ids }: { ids: string[] }) => {
      binned = binned.filter(x => !ids.includes(x.id));
      return null;
    },
    trash_empty: () => { binned = []; return null; },
  });
}

async function openTrash(page: Page) {
  await page.getByRole("button", { name: "Open menu" }).first().click();
  await page.locator("aside.drawer").getByRole("button", { name: "Trash" }).click();
  await expect(page.locator(".trash-title")).toBeVisible();
}

async function openNotes(page: Page) {
  await page.getByRole("button", { name: "Open menu" }).first().click();
  await page.locator("aside.drawer").getByRole("button", { name: "All notes" }).click();
}

const card = (page: Page, title: string) => page.locator(".note-card", { hasText: title });

test("a deleted note goes to Trash and can be restored", async ({ page }) => {
  await withTrash(page);
  await page.goto("/");

  await card(page, "Meeting notes").getByRole("button", { name: "More options" }).click();
  await page.locator(".popover-item", { hasText: "Delete" }).click();
  await expect(page.locator(".modal")).toContainText("moved to Trash");
  await page.locator(".modal").getByRole("button", { name: "Move to Trash" }).click();
  await expect(card(page, "Meeting notes")).toHaveCount(0);

  await openTrash(page);
  await expect(card(page, "Meeting notes")).toContainText("Deleted");
  await page.getByRole("button", { name: "Restore Meeting notes" }).click();
  await expect(card(page, "Meeting notes")).toHaveCount(0);
  await expect(page.getByText("Trash is empty.")).toBeVisible();

  await openNotes(page);
  await expect(card(page, "Meeting notes")).toBeVisible();
});

test("delete forever removes a note from Trash for good", async ({ page }) => {
  await withTrash(page);
  await page.goto("/");

  await card(page, "Shopping list").getByRole("button", { name: "More options" }).click();
  await page.locator(".popover-item", { hasText: "Delete" }).click();
  await page.locator(".modal").getByRole("button", { name: "Move to Trash" }).click();

  await openTrash(page);
  await page.getByRole("button", { name: "Delete Shopping list forever" }).click();
  await page.locator(".modal").getByRole("button", { name: "Delete forever" }).click();
  await expect(page.getByText("Trash is empty.")).toBeVisible();

  await openNotes(page);
  await expect(card(page, "Shopping list")).toHaveCount(0);
  await expect(card(page, "Meeting notes")).toBeVisible();
});
