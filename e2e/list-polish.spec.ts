import { test, expect, type Page } from "playwright/test";
import { setupTauriMock, reject, MOCK_NOTES, MOCK_NOTE_DETAIL } from "./mock";

const WORK = { id: "f-work", parent_id: null, name: "Work", note_count: 0 };
const HOME = { id: "f-home", parent_id: null, name: "Home", note_count: 0 };

const card = (page: Page, title: string) => page.locator(".note-card", { hasText: title }).first();
const menuItem = (page: Page, label: string) => page.locator(".popover-item", { hasText: label });

test.describe("the row menu sits at the far right", () => {
  for (const [name, size] of [["phone", { width: 390, height: 844 }], ["desktop", { width: 1280, height: 800 }]] as const) {
    test(name, async ({ page }) => {
      await setupTauriMock(page, { folder_list: [WORK] });
      await page.setViewportSize(size);
      await page.goto("/");
      // Until the icon font loads, an icon renders as its ligature text ("chevron_right"),
      // which is far wider than the glyph and throws the measurements below.
      await page.waitForFunction(() => document.fonts.check('24px "Material Symbols Outlined"'));
      for (const [row, other] of [[card(page, "Meeting notes"), ".date"], [card(page, "Work"), ".chevron"]] as const) {
        const kebab = (await row.locator(".card-menu").boundingBox())!;
        const before = (await row.locator(other).boundingBox())!;
        expect(kebab.x).toBeGreaterThanOrEqual(before.x + before.width - 1);
        const rowBox = (await row.boundingBox())!;
        expect(rowBox.x + rowBox.width - (kebab.x + kebab.width)).toBeLessThan(24);
      }
    });
  }
});

test.describe("copy, cut and paste", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
  });

  test("a copied note is pasted into the folder being viewed", async ({ page }) => {
    const calls: unknown[] = [];
    await setupTauriMock(page, {
      folder_list: [WORK],
      notes_copy: (args: unknown) => { calls.push(args); return { copied: ["n-new"], skipped_locked: 0 }; },
    });
    await page.goto("/");
    await card(page, "Meeting notes").click({ button: "right" });
    await menuItem(page, "Copy").click();
    await card(page, "Work").click();
    await expect(page.locator(".crumb.current")).toHaveText("Work");
    await page.keyboard.press("Control+v");
    await expect.poll(() => calls).toEqual([{ ids: ["note-1"], folderId: "f-work" }]);
  });

  test("Ctrl+C copies the selection", async ({ page }) => {
    const calls: unknown[] = [];
    await setupTauriMock(page, {
      notes_copy: (args: unknown) => { calls.push(args); return { copied: ["a", "b"], skipped_locked: 0 }; },
    });
    await page.goto("/");
    await page.getByRole("button", { name: "Select notes" }).click();
    await card(page, "Meeting notes").click();
    await card(page, "Draft").click();
    await page.keyboard.press("Control+c");
    await page.getByRole("button", { name: "Paste here" }).click();
    await expect.poll(() => calls).toEqual([{ ids: ["note-1", "note-3"], folderId: null }]);
  });

  test("a cut note looks dimmed, and pasting moves it", async ({ page }) => {
    const calls: unknown[] = [];
    await setupTauriMock(page, {
      folder_list: [WORK],
      note_set_folder: (args: unknown) => { calls.push(args); return null; },
    });
    await page.goto("/");
    await card(page, "Draft").click({ button: "right" });
    await menuItem(page, "Cut").click();
    await expect(card(page, "Draft")).toHaveClass(/\bcut\b/);
    await card(page, "Work").click({ button: "right" });
    await menuItem(page, "Paste into").click();
    await expect.poll(() => calls).toEqual([{ noteId: "note-3", folderId: "f-work" }]);
    await expect(page.getByRole("button", { name: "Paste here" })).toHaveCount(0);
  });

  test("Esc clears a cut", async ({ page }) => {
    await setupTauriMock(page);
    await page.goto("/");
    await card(page, "Draft").click({ button: "right" });
    await menuItem(page, "Cut").click();
    await expect(card(page, "Draft")).toHaveClass(/\bcut\b/);
    await page.keyboard.press("Escape");
    await expect(card(page, "Draft")).not.toHaveClass(/\bcut\b/);
  });

  test("folders copy and move as a whole", async ({ page }) => {
    const copies: unknown[] = [];
    const moves: unknown[] = [];
    await setupTauriMock(page, {
      folder_list: [WORK, HOME],
      folder_copy: (args: unknown) => { copies.push(args); return { copied: [], skipped_locked: 0 }; },
      folder_move: (args: unknown) => { moves.push(args); return null; },
    });
    await page.goto("/");
    await card(page, "Work").click({ button: "right" });
    await menuItem(page, "Copy").click();
    await card(page, "Home").click();
    await page.keyboard.press("Control+v");
    await expect.poll(() => copies).toEqual([{ id: "f-work", parentId: "f-home" }]);

    await page.locator(".crumb", { hasText: "Home" }).first().click();
    await card(page, "Home").click({ button: "right" });
    await menuItem(page, "Cut").click();
    await card(page, "Work").click({ button: "right" });
    await menuItem(page, "Paste into").click();
    await expect.poll(() => moves).toEqual([{ id: "f-home", parentId: "f-work" }]);
  });

  test("locked notes left out of a copy are reported", async ({ page }) => {
    await setupTauriMock(page, { notes_copy: { copied: [], skipped_locked: 1 } });
    await page.goto("/");
    await card(page, "Draft").click({ button: "right" });
    await menuItem(page, "Copy").click();
    await page.getByRole("button", { name: "Paste here" }).click();
    await expect(page.getByRole("alert")).toContainText("Unlock it first to copy");
  });
});

test.describe("find in note", () => {
  test("Ctrl+F in a document opens the find bar, not the list search", async ({ page }) => {
    // The real backend lists legacy markdown notes as documents; the shared mock
    // still says "markdown", which mounts no rich editor.
    await setupTauriMock(page, { note_get: { ...MOCK_NOTE_DETAIL, kind: "document" } });
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/note/note-1");
    await page.locator(".ProseMirror").click();
    await page.keyboard.press("Control+f");
    const box = page.getByRole("searchbox", { name: "Find in note" });
    await expect(box).toBeFocused();
    await expect(page.locator(".search")).not.toBeFocused();
    await box.fill("item");
    await expect(page.locator(".find-count")).toHaveText("1 of 2");
    await box.press("Enter");
    await expect(page.locator(".find-count")).toHaveText("2 of 2");
    await box.press("Shift+Enter");
    await expect(page.locator(".find-count")).toHaveText("1 of 2");
    await box.press("Escape");
    await expect(box).toHaveCount(0);
    expect(await page.evaluate(() => CSS.highlights.size)).toBe(0);
  });

  test("finds text inside a checklist's fields on a phone", async ({ page }) => {
    await setupTauriMock(page, {
      note_get: {
        ...MOCK_NOTE_DETAIL, kind: "checklist", title: "Shopping list",
        content: { items: [
          { id: "1", text: "Milk", checked: false, children: [] },
          { id: "2", text: "Oat milk", checked: false, children: [] },
          { id: "3", text: "Bread", checked: false, children: [] },
        ] },
      },
    });
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/note/note-2");
    await expect(page.locator(".title-input")).toHaveValue("Shopping list");
    await page.keyboard.press("Control+f");
    const box = page.getByRole("searchbox", { name: "Find in note" });
    await box.fill("milk");
    await expect(page.locator(".find-count")).toHaveText("1 of 2");
    await page.getByRole("button", { name: "Next match" }).click();
    await expect(page.locator(".find-count")).toHaveText("2 of 2");
  });

  test("Ctrl+F with focus in the list still searches the list", async ({ page }) => {
    await setupTauriMock(page);
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/note/note-1");
    await expect(page.locator(".title-input")).toHaveValue("Meeting notes");
    await card(page, "Draft").focus();
    await page.keyboard.press("Control+f");
    await expect(page.locator(".search")).toBeFocused();
    await expect(page.getByRole("searchbox", { name: "Find in note" })).toHaveCount(0);
  });
});

test.describe("autosave", () => {
  test.beforeEach(async ({ page }) => {
    await page.setViewportSize({ width: 1280, height: 800 });
  });

  test("the setting persists", async ({ page }) => {
    const sets: unknown[] = [];
    await setupTauriMock(page, { set_autosave: (args: unknown) => { sets.push(args); return null; } });
    await page.goto("/settings");
    const toggle = page.getByRole("switch", { name: "Autosave" });
    await expect(toggle).toHaveAttribute("aria-checked", "false");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-checked", "true");
    await expect.poll(() => sets).toEqual([{ enabled: true }]);
  });

  test("the saved setting is shown on load", async ({ page }) => {
    await setupTauriMock(page, { get_autosave: true });
    await page.goto("/settings");
    await expect(page.getByRole("switch", { name: "Autosave" })).toHaveAttribute("aria-checked", "true");
  });

  test("on: an edit saves itself, and leaving does not ask", async ({ page }) => {
    const updates: { id: string; input: { title: string } }[] = [];
    await setupTauriMock(page, {
      get_autosave: true,
      note_update: (args: { id: string; input: { title: string } }) => { updates.push(args); return MOCK_NOTES[0]; },
    });
    await page.goto("/note/note-1");
    await page.fill(".title-input", "Edited once");
    await expect.poll(() => updates.length, { timeout: 4000 }).toBe(1);
    expect(updates[0].input.title).toBe("Edited once");

    await page.fill(".title-input", "Edited twice");
    await card(page, "Draft").click();
    await page.waitForURL(/note-3/);
    await expect(page.locator(".modal")).toHaveCount(0);
    expect(updates.at(-1)!.input.title).toBe("Edited twice");
  });

  test("off: leaving with an edit asks first", async ({ page }) => {
    await setupTauriMock(page);
    await page.goto("/note/note-1");
    await page.fill(".title-input", "Edited");
    await card(page, "Draft").click();
    await expect(page.locator(".modal")).toContainText("Unsaved changes");
  });
});

test.describe("closing a note on desktop", () => {
  const empty = (page: Page) => page.getByText("Select a note to read it, or create a new one.");
  const inWork = (page: Page) => page.locator(".crumb.current", { hasText: "Work" });

  test("the close button returns to the empty view", async ({ page }) => {
    await setupTauriMock(page);
    await page.goto("/note/note-1");
    await page.getByRole("link", { name: "Close note" }).click();
    await expect(empty(page)).toBeVisible();
  });

  test("opening a folder closes the note, asking about unsaved edits first", async ({ page }) => {
    await setupTauriMock(page, { folder_list: [WORK] });
    await page.goto("/note/note-1");
    await page.fill(".title-input", "Edited");

    await card(page, "Work").click();
    await expect(page.locator(".modal")).toContainText("Unsaved changes");
    await page.locator(".modal .btn-cancel").click();
    await expect(page).toHaveURL(/\/note\/note-1/);
    await expect(inWork(page)).toHaveCount(0);

    await card(page, "Work").click();
    await page.locator(".modal .btn-alt", { hasText: "Discard" }).click();
    await expect(empty(page)).toBeVisible();
    await expect(inWork(page)).toBeVisible();
    await expect(page).toHaveURL(/\/$/);
  });

  test("with autosave on, opening a folder saves and closes without asking", async ({ page }) => {
    const updates: unknown[] = [];
    await setupTauriMock(page, {
      folder_list: [WORK],
      get_autosave: true,
      note_update: (args: unknown) => { updates.push(args); return MOCK_NOTES[0]; },
    });
    await page.goto("/note/note-1");
    await page.fill(".title-input", "Edited");
    await card(page, "Work").click();
    await expect(empty(page)).toBeVisible();
    await expect(inWork(page)).toBeVisible();
    await expect(page.locator(".modal")).toHaveCount(0);
    expect(updates.length).toBeGreaterThan(0);
  });
});

test("Ctrl+F with no note open focuses the list search", async ({ page }) => {
  await setupTauriMock(page);
  await page.goto("/note/note-1");
  await page.getByRole("link", { name: "Close note" }).click();
  await expect(page.getByText("Select a note to read it, or create a new one.")).toBeVisible();
  await page.keyboard.press("Control+f");
  await expect(page.getByRole("textbox", { name: "Search notes" })).toBeFocused();
});

test("leaving during an in-flight autosave create does not create the note twice", async ({ page }) => {
  let creates = 0;
  const updates: string[] = [];
  await setupTauriMock(page, {
    get_autosave: true,
    note_create: async () => {
      creates++;
      await new Promise(r => setTimeout(r, 1500));
      return { ...MOCK_NOTES[0], id: "made-1" };
    },
    note_update: (args: { id: string }) => { updates.push(args.id); return MOCK_NOTES[0]; },
  });
  await page.goto("/note/new?kind=document");
  await page.fill(".title-input", "Fresh");
  // Autosave fires after 1s and its create takes 1.5s; leaving lands in between,
  // and with autosave on, leaving saves too.
  await page.waitForTimeout(1300);
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.waitForURL(/note-3/, { timeout: 6000 });
  expect(creates).toBe(1);
  expect(updates.every(id => id === "made-1")).toBe(true);
});

test("autosave on blur keeps a half-typed tag and leaves the note saved", async ({ page }) => {
  const updates: { input: { tags: string[] } }[] = [];
  await setupTauriMock(page, {
    get_autosave: true,
    note_update: (args: { input: { tags: string[] } }) => { updates.push(args); return MOCK_NOTES[0]; },
  });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Edited");
  await page.fill(".tag-input", "urgent");
  await page.evaluate(() => {
    document.hasFocus = () => false;
    window.dispatchEvent(new Event("blur"));
  });
  await expect.poll(() => updates.length).toBe(1);
  expect(updates[0].input.tags).toContain("urgent");
  // A stale baseline would leave the note dirty and queue another save.
  await page.waitForTimeout(2000);
  expect(updates.length).toBe(1);
});

test("Delete acts on the focused row, not the note open beside the list", async ({ page }) => {
  const deleted: string[] = [];
  await setupTauriMock(page, { notes_delete: (args: { ids: string[] }) => { deleted.push(...args.ids); return null; } });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.locator(".note-card", { hasText: "Draft" }).first().focus();
  await page.keyboard.press("Delete");
  await page.locator(".modal .btn-confirm").click();
  await expect.poll(() => deleted).toEqual(["note-3"]);
});

test("opening Trash with unsaved edits waits for the prompt; Cancel changes nothing", async ({ page }) => {
  await setupTauriMock(page);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Edited");
  const openTrash = async () => {
    await page.getByRole("button", { name: "Open menu" }).first().click();
    await page.locator("aside.drawer").getByRole("button", { name: "Trash" }).click();
  };
  await openTrash();
  await page.locator(".modal .btn-cancel").click();
  await expect(page).toHaveURL(/\/note\/note-1/);
  await expect(page.locator(".note-card", { hasText: "Meeting notes" }).first()).toBeVisible();

  await openTrash();
  await page.locator(".modal .btn-alt", { hasText: "Discard" }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.locator(".note-card", { hasText: "Meeting notes" })).toHaveCount(0);
});

test("a failed delete is reported, not swallowed", async ({ page }) => {
  await setupTauriMock(page, { notes_delete: () => reject("database is locked") });
  await page.goto("/");
  await page.locator(".note-card", { hasText: "Draft" }).first().getByRole("button", { name: "More options" }).click();
  await page.locator(".popover-item", { hasText: "Delete" }).click();
  await page.locator(".modal .btn-confirm").click();
  await expect(page.getByRole("alert")).toContainText("database is locked");
});
