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
  expect(updates.length).toBeGreaterThanOrEqual(1);
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

test("a click during an autosave leave becomes the destination; the note is made once and saves stay put", async ({ page }) => {
  let creates = 0;
  const updates: string[] = [];
  await setupTauriMock(page, {
    get_autosave: true,
    note_create: async () => {
      creates++;
      await new Promise(r => setTimeout(r, 2500));
      return { ...MOCK_NOTES[0], id: "made-1" };
    },
    note_update: (args: { id: string }) => { updates.push(args.id); return MOCK_NOTES[2]; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/new?kind=document");
  await page.fill(".title-input", "Fresh");
  await page.waitForTimeout(1300);
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  // The leave to Draft is still saving the new note; this click replaces its target.
  await page.locator(".note-card", { hasText: "Shopping list" }).first().click();
  await page.waitForURL(/note-2/, { timeout: 8000 });
  await page.waitForTimeout(1000);
  await expect(page).toHaveURL(/note-2/);
  expect(creates).toBe(1);

  await page.fill(".title-input", "Shopping list, edited");
  await page.keyboard.press("Control+s");
  await expect.poll(() => updates.at(-1)).toBe("note-2");
});

test("autosave saves a half-typed tag on its own when the window loses focus", async ({ page }) => {
  const updates: { input: { tags: string[] } }[] = [];
  await setupTauriMock(page, {
    get_autosave: true,
    note_update: (args: { input: { tags: string[] } }) => { updates.push(args); return MOCK_NOTES[0]; },
  });
  await page.goto("/note/note-1");
  await page.fill(".tag-input", "urgent");
  await page.evaluate(() => {
    document.hasFocus = () => false;
    window.dispatchEvent(new Event("blur"));
  });
  await expect.poll(() => updates.length).toBe(1);
  expect(updates[0].input.tags).toContain("urgent");
});

test("a failed multi-note delete keeps the selection for a retry", async ({ page }) => {
  await setupTauriMock(page, { notes_delete: () => reject("database is locked") });
  await page.goto("/");
  await page.getByRole("button", { name: "Select notes" }).first().click();
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.locator(".note-card", { hasText: "Shopping list" }).first().click();
  await page.keyboard.press("Delete");
  await page.locator(".modal .btn-confirm").click();
  await expect(page.getByRole("alert")).toContainText("database is locked");
  await expect(page.locator(".sel-count")).toContainText("2 selected");
});

test("deleting a one-note selection leaves select mode", async ({ page }) => {
  await setupTauriMock(page);
  await page.goto("/");
  await page.getByRole("button", { name: "Select notes" }).first().click();
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.keyboard.press("Delete");
  await page.locator(".modal .btn-confirm").click();
  await expect(page.locator(".sel-count")).toHaveCount(0);
});

test("while autosave is leaving a note, a further click waits for the save and becomes the destination", async ({ page }) => {
  const updates: { id: string; input: { title: string } }[] = [];
  await setupTauriMock(page, {
    get_autosave: true,
    note_update: async (args: { id: string; input: { title: string } }) => {
      await new Promise(r => setTimeout(r, 1500));
      updates.push(args);
      return MOCK_NOTES[0];
    },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Edited before leaving");
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.locator(".note-card", { hasText: "Shopping list" }).first().click();
  await page.waitForURL(/note-2/, { timeout: 6000 });
  expect(updates.some(u => u.id === "note-1" && u.input.title === "Edited before leaving")).toBe(true);
});

test("saving while the next note loads never writes the last note into it", async ({ page }) => {
  const updates: { id: string; input: { title: string } }[] = [];
  await setupTauriMock(page, {
    note_get: async (args: { id: string }) => {
      if (args.id === "note-3") await new Promise(r => setTimeout(r, 2000));
      return { ...MOCK_NOTE_DETAIL, id: args.id, title: args.id === "note-3" ? "Draft" : "Meeting notes" };
    },
    note_update: (args: { id: string; input: { title: string } }) => { updates.push(args); return MOCK_NOTES[0]; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Discard me");
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.locator(".modal .btn-alt", { hasText: "Discard" }).click();
  await page.waitForURL(/note-3/);
  await page.keyboard.press("Control+s");
  await expect(page.locator(".title-input")).toHaveValue("Draft", { timeout: 5000 });
  expect(updates.filter(u => u.id === "note-3" && u.input.title === "Discard me")).toEqual([]);
});

test("a failure from the note just left does not prompt on the next one", async ({ page }) => {
  await setupTauriMock(page, {
    note_update: async () => {
      await new Promise(r => setTimeout(r, 1500));
      return reject("locked");
    },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Edited");
  // A slow save is in flight; leaving with Discard does not wait for it.
  await page.keyboard.press("Control+s");
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.locator(".modal .btn-alt", { hasText: "Discard" }).click();
  await page.waitForURL(/note-3/);
  await page.waitForTimeout(2500);
  await expect(page.getByText("Unlock to save")).toHaveCount(0);
});

test("a draft pending for one note is never saved under the next", async ({ page }) => {
  const drafts: string[] = [];
  await setupTauriMock(page, {
    note_draft_save: (args: { id: string }) => { drafts.push(args.id); return null; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Typed");
  await page.keyboard.press("Control+s");
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.waitForURL(/note-3/);
  await page.waitForTimeout(1500);
  expect(drafts).not.toContain("note-3");
});


test("double-clicking the same note during an autosave leave saves the left note and edits the next", async ({ page }) => {
  const updates: { id: string; input: { title: string } }[] = [];
  await setupTauriMock(page, {
    get_autosave: true,
    note_update: async (args: { id: string; input: { title: string } }) => {
      await new Promise(r => setTimeout(r, 1200));
      updates.push(args);
      return MOCK_NOTES[0];
    },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Meeting, edited");
  const draft = page.locator(".note-card", { hasText: "Draft" }).first();
  await draft.click();
  await draft.click();
  await page.waitForURL(/note-3/, { timeout: 8000 });
  expect(updates.some(u => u.id === "note-1" && u.input.title === "Meeting, edited")).toBe(true);

  // The next note starts clean and still saves its own edits.
  await page.fill(".title-input", "Draft, edited");
  await page.locator(".note-card", { hasText: "Shopping list" }).first().click();
  await page.waitForURL(/note-2/, { timeout: 8000 });
  expect(updates.some(u => u.id === "note-3" && u.input.title === "Draft, edited")).toBe(true);
});

test("a draft banner belongs to its note, not the next one", async ({ page }) => {
  await setupTauriMock(page, {
    note_draft_get: (args: { id: string }) =>
      args.id === "note-1"
        ? { title: "Meeting notes, unsaved", content: { body: "x" }, tags: [], updated_at: 1700000000 }
        : null,
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await expect(page.locator(".draft-banner")).toBeVisible();
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.waitForURL(/note-3/);
  await expect(page.locator(".draft-banner")).toHaveCount(0);
});

test("a note that failed to load is never saved over", async ({ page }) => {
  const updates: string[] = [];
  await setupTauriMock(page, {
    note_get: (args: { id: string }) =>
      args.id === "note-3" ? reject("database is locked") : { ...MOCK_NOTE_DETAIL, id: args.id },
    note_update: (args: { id: string }) => { updates.push(args.id); return MOCK_NOTES[0]; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.waitForURL(/note-3/);
  await expect(page.getByText("database is locked")).toBeVisible();
  await page.keyboard.press("Control+s");
  await page.waitForTimeout(800);
  expect(updates).not.toContain("note-3");
});

test("saving on a click to the open note keeps tracking later edits", async ({ page }) => {
  await setupTauriMock(page);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "First edit");
  await page.locator(".note-card", { hasText: "Meeting notes" }).first().click();
  await page.locator(".modal .btn-confirm").click();
  await expect(page.locator(".modal")).toHaveCount(0);
  // A later edit must still count as unsaved.
  await page.fill(".title-input", "Second edit");
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await expect(page.locator(".modal")).toContainText("Unsaved changes");
});

test("a note that failed to load shows an error, not an editor", async ({ page }) => {
  await setupTauriMock(page, {
    note_get: (args: { id: string }) =>
      args.id === "note-3" ? reject("database is locked") : { ...MOCK_NOTE_DETAIL, id: args.id },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-3");
  await expect(page.getByText("Couldn't open this note")).toBeVisible();
  await expect(page.locator(".title-input")).toHaveCount(0);
  await page.getByRole("link", { name: "Back to notes" }).click();
  await expect(page).toHaveURL(/\/$/);
});

test("Back during an autosave leave is held, not replayed as a new navigation", async ({ page }) => {
  const updates: string[] = [];
  await setupTauriMock(page, {
    get_autosave: true,
    note_update: async (args: { id: string }) => {
      await new Promise(r => setTimeout(r, 1500));
      updates.push(args.id);
      return MOCK_NOTES[0];
    },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-3");
  await page.locator(".note-card", { hasText: "Meeting notes" }).first().click();
  await page.waitForURL(/note-1/);
  const before = await page.evaluate(() => history.length);
  await page.fill(".title-input", "Edited");
  await page.locator(".note-card", { hasText: "Shopping list" }).first().click();
  await page.goBack();
  await page.waitForURL(/note-2/, { timeout: 8000 });
  expect(updates).toContain("note-1");
  expect(await page.evaluate(() => history.length)).toBe(before + 1);
});


test("opening a note from the list loads it once, and re-clicking it keeps the editor", async ({ page }) => {
  const gets: string[] = [];
  await setupTauriMock(page, {
    note_get: (args: { id: string }) => { gets.push(args.id); return { ...MOCK_NOTE_DETAIL, id: args.id }; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/");
  await page.locator(".note-card", { hasText: "Meeting notes" }).first().click();
  await expect(page.locator(".title-input")).toHaveValue("Meeting notes");
  await page.waitForTimeout(500);
  expect(gets.filter(g => g === "note-1").length).toBe(1);
  await page.locator(".note-card", { hasText: "Meeting notes" }).first().click();
  await page.waitForTimeout(500);
  expect(gets.filter(g => g === "note-1").length).toBe(1);
});

test("Ctrl+S on a new note binds it without rebuilding the editor", async ({ page }) => {
  const gets: string[] = [];
  await setupTauriMock(page, {
    note_get: (args: { id: string }) => { gets.push(args.id); return { ...MOCK_NOTE_DETAIL, id: args.id }; },
    note_create: () => ({ ...MOCK_NOTES[0], id: "made-1" }),
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/new?kind=document");
  await page.fill(".title-input", "Fresh");
  await page.keyboard.press("Control+s");
  await page.waitForURL(/made-1/);
  await page.waitForTimeout(500);
  expect(gets).not.toContain("made-1");
  await expect(page.locator(".title-input")).toHaveValue("Fresh");
});

test("a note deleted from the editor leaves the list selection", async ({ page }) => {
  let deleted = false;
  await setupTauriMock(page, {
    note_list: () => (deleted ? MOCK_NOTES.filter(n => n.id !== "note-1") : MOCK_NOTES),
    // Like the backend, the count leaves Trash out.
    note_count: () => (deleted ? MOCK_NOTES.length - 1 : MOCK_NOTES.length),
    notes_delete: () => { deleted = true; return null; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.getByRole("button", { name: "Select notes" }).first().click();
  await page.locator(".note-card", { hasText: "Meeting notes" }).first().click();
  await page.locator(".note-card", { hasText: "Shopping list" }).first().click();
  await expect(page.locator(".sel-count")).toContainText("2 selected");
  await page.locator('.editor-header button[aria-label="More options"]').click();
  await page.locator(".overflow-item", { hasText: "Delete" }).click();
  await page.locator(".modal .btn-confirm").click();
  await expect(page.locator(".sel-count")).toContainText("1 selected");
});


test("a new note saved on the way to a fresh new note gets created, and the fresh one starts blank", async ({ page }) => {
  let creates = 0;
  await setupTauriMock(page, {
    get_autosave: true,
    note_create: () => { creates++; return { ...MOCK_NOTES[0], id: "made-1" }; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/new?kind=document");
  await page.fill(".title-input", "First new note");
  await page.locator(".compose-btn").click();
  await page.locator('[role="dialog"][aria-label="New note"]').getByText("Document").click();
  await expect.poll(() => creates).toBe(1);
  await expect(page.locator(".title-input")).toHaveValue("");
  await expect(page).toHaveURL(/\/note\/new/);
});

test("Discard means the edits are gone, not offered back as a draft", async ({ page }) => {
  let draft: unknown = null;
  await setupTauriMock(page, {
    note_draft_save: (args: { draft: { title: string } }) => {
      draft = { ...args.draft, updated_at: 1700000000 };
      return null;
    },
    note_draft_get: () => draft,
    note_draft_discard: () => { draft = null; return null; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Throw this away");
  await expect.poll(() => draft !== null, { timeout: 3000 }).toBe(true);
  await page.locator(".note-card", { hasText: "Meeting notes" }).first().click();
  await page.locator(".modal .btn-alt", { hasText: "Discard" }).click();
  await expect(page.locator(".title-input")).toHaveValue("Meeting notes");
  await expect(page.locator(".draft-banner")).toHaveCount(0);
});

test("text typed while a save is running is still unsaved afterwards", async ({ page }) => {
  await setupTauriMock(page, {
    note_update: async () => {
      await new Promise(r => setTimeout(r, 1000));
      return MOCK_NOTES[0];
    },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Saved part");
  await page.keyboard.press("Control+s");
  await page.fill(".title-input", "Saved part, then more");
  await page.waitForTimeout(1500);
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await expect(page.locator(".modal")).toContainText("Unsaved changes");
});

test("clicking a note that failed to load tries again", async ({ page }) => {
  let attempts = 0;
  await setupTauriMock(page, {
    note_get: (args: { id: string }) => {
      if (args.id !== "note-3") return { ...MOCK_NOTE_DETAIL, id: args.id };
      attempts++;
      return attempts === 1 ? reject("database is locked") : { ...MOCK_NOTE_DETAIL, id: "note-3", title: "Draft" };
    },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-3");
  await expect(page.getByText("Couldn't open this note")).toBeVisible();
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await expect(page.locator(".title-input")).toHaveValue("Draft");
});


test("Save on the way out also writes what was typed while it ran", async ({ page }) => {
  const titles: string[] = [];
  await setupTauriMock(page, {
    note_update: async (args: { input: { title: string } }) => {
      await new Promise(r => setTimeout(r, 1000));
      titles.push(args.input.title);
      return MOCK_NOTES[0];
    },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.fill(".title-input", "Before saving");
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.locator(".modal .btn-confirm").click();
  await page.fill(".title-input", "Before saving, and after");
  await page.waitForURL(/note-3/, { timeout: 8000 });
  expect(titles.at(-1)).toBe("Before saving, and after");
});

test("re-clicking the open note mid-autosave saves in place without rebuilding", async ({ page }) => {
  const gets: string[] = [];
  const updates: string[] = [];
  await setupTauriMock(page, {
    get_autosave: true,
    note_get: (args: { id: string }) => { gets.push(args.id); return { ...MOCK_NOTE_DETAIL, id: args.id }; },
    note_update: (args: { id: string }) => { updates.push(args.id); return MOCK_NOTES[0]; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await expect(page.locator(".title-input")).toHaveValue("Meeting notes");
  await page.fill(".title-input", "Typing");
  await page.locator(".note-card", { hasText: "Meeting notes" }).first().click();
  await expect.poll(() => updates.length).toBeGreaterThan(0);
  expect(gets.filter(g => g === "note-1").length).toBe(1);
  await expect(page.locator(".title-input")).toHaveValue("Typing");
});

test("Discard keeps an earlier session's draft when this session wrote none", async ({ page }) => {
  let discards = 0;
  await setupTauriMock(page, {
    note_draft_get: (args: { id: string }) =>
      args.id === "note-1"
        ? { title: "From last session", content: { body: "x" }, tags: [], updated_at: 1700000000 }
        : null,
    note_draft_discard: () => { discards++; return null; },
  });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await expect(page.locator(".draft-banner")).toBeVisible();
  await page.fill(".tag-input", "half");
  await page.locator(".note-card", { hasText: "Draft" }).first().click();
  await page.locator(".modal .btn-alt", { hasText: "Discard" }).click();
  await page.waitForURL(/note-3/);
  expect(discards).toBe(0);
});

test("a note trashed from the editor leaves the selection even when the list is capped", async ({ page }) => {
  await setupTauriMock(page, { note_count: 999 });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/note/note-1");
  await page.getByRole("button", { name: "Select notes" }).first().click();
  await page.locator(".note-card", { hasText: "Meeting notes" }).first().click();
  await page.locator(".note-card", { hasText: "Shopping list" }).first().click();
  await expect(page.locator(".sel-count")).toContainText("2 selected");
  await page.locator('.editor-header button[aria-label="More options"]').click();
  await page.locator(".overflow-item", { hasText: "Delete" }).click();
  await page.locator(".modal .btn-confirm").click();
  await expect(page.locator(".sel-count")).toContainText("1 selected");
});
