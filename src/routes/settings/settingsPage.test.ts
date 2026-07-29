// @vitest-environment happy-dom
//
// Settings owns backup and restore, which is the only path that can overwrite a
// whole library at once. The password gate for sealed backups matters most: a
// backup carrying protected notes must not be imported without it.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import { writable } from "svelte/store";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "0.4.0") }));

vi.mock("$lib/stores/theme", () => ({
  theme: writable("candy-light"),
  toggleDarkMode: vi.fn(),
}));
vi.mock("$lib/stores/sidebar", () => ({ sidebarOpen: writable(false) }));

vi.mock("$lib/tauri", () => ({
  getDeviceName: vi.fn(async () => "Workstation"),
  setDeviceName: vi.fn(async () => {}),
  startReceiving: vi.fn(async () => {}),
  stopReceiving: vi.fn(async () => {}),
  isReceiving: vi.fn(async () => false),
  deviceIps: vi.fn(async () => ["192.168.1.10"]),
  notesExport: vi.fn(async () => '{"notes":[]}'),
  notesImport: vi.fn(async () => ({ imported: 0, updated: 0, skipped: 0, errors: [] })),
  WRONG_PASSWORD: "WRONG_PASSWORD",
}));

import {
  getDeviceName, setDeviceName, startReceiving, stopReceiving, isReceiving,
  notesExport, notesImport,
} from "$lib/tauri";
import SettingsPage from "./+page.svelte";

let cleanup: (() => void) | null = null;
const flush = async () => { await Promise.resolve(); await new Promise(r => setTimeout(r, 0)); };
const settle = async () => { for (let i = 0; i < 6; i++) await flush(); };

async function setup() {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(SettingsPage, { target, props: {} });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  await settle();
  return target;
}

const rowByTitle = (t: HTMLElement, text: RegExp) =>
  [...t.querySelectorAll<HTMLElement>(".row")]
    .find(r => text.test(r.querySelector(".row-title")?.textContent ?? ""));

const status = (t: HTMLElement) => t.textContent ?? "";

/// Drive the hidden file picker the way choosing a file does.
async function pickFile(t: HTMLElement, contents: string, name = "backup.json") {
  const input = t.querySelector<HTMLInputElement>('input[type="file"]')!;
  const file = new File([contents], name, { type: "application/json" });
  Object.defineProperty(input, "files", { value: [file], configurable: true });
  input.dispatchEvent(new Event("change", { bubbles: true }));
  await settle();
}

const confirmImport = async () => {
  document.querySelector<HTMLButtonElement>(".btn-confirm")!.click();
  await settle();
};

let clicked: HTMLAnchorElement[] = [];

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(getDeviceName).mockResolvedValue("Workstation");
  vi.mocked(isReceiving).mockResolvedValue(false);
  vi.mocked(notesExport).mockResolvedValue('{"notes":[]}');
  vi.mocked(notesImport).mockResolvedValue({ imported: 0, updated: 0, skipped: 0, errors: [] } as never);

  clicked = [];
  vi.stubGlobal("URL", Object.assign(URL, {
    createObjectURL: vi.fn(() => "blob:fake"),
    revokeObjectURL: vi.fn(),
  }));
  // jsdom/happy-dom would try to navigate on an anchor click
  vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(function (this: HTMLAnchorElement) {
    clicked.push(this);
  });
});

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("first paint", () => {
  it("shows the device name it read from the backend", async () => {
    const t = await setup();
    expect(getDeviceName).toHaveBeenCalled();
    expect(status(t)).toContain("Workstation");
  });

  // The address is only useful to a sender while this device is listening, so
  // it stays hidden until then.
  it("hides the IP addresses until receiving is switched on", async () => {
    const t = await setup();
    expect(status(t)).not.toContain("192.168.1.10");
  });

  it("shows them once receiving is active", async () => {
    vi.mocked(isReceiving).mockResolvedValue(true);
    const t = await setup();
    expect(status(t)).toContain("192.168.1.10");
  });

  it("still renders when the backend calls fail", async () => {
    vi.mocked(getDeviceName).mockRejectedValue(new Error("no keychain"));
    vi.mocked(isReceiving).mockRejectedValue(new Error("no net"));
    const t = await setup();
    expect(t.querySelector(".settings-body")).toBeTruthy();
    expect(status(t)).not.toContain("no keychain");
  });
});

describe("export", () => {
  it("downloads a dated backup file", async () => {
    const t = await setup();
    rowByTitle(t, /Export all notes/)!.click();
    await settle();

    expect(notesExport).toHaveBeenCalledWith("0.4.0");
    expect(clicked).toHaveLength(1);
    expect(clicked[0].download).toMatch(/^panote-backup-\d{4}-\d{2}-\d{2}\.json$/);
    expect(status(t)).toContain("Backup downloaded.");
  });

  it("reports a failure instead of silently doing nothing", async () => {
    vi.mocked(notesExport).mockRejectedValue(new Error("db locked"));
    const t = await setup();
    rowByTitle(t, /Export all notes/)!.click();
    await settle();

    expect(clicked).toHaveLength(0);
    expect(status(t)).toContain("Export failed");
    expect(status(t)).toContain("db locked");
  });
});

describe("import", () => {
  it("confirms before overwriting anything", async () => {
    const t = await setup();
    await pickFile(t, '{"notes":[{"id":"a"}]}');

    expect(document.querySelector(".modal")).toBeTruthy();
    expect(notesImport).not.toHaveBeenCalled();
  });

  it("imports once confirmed and reports the summary", async () => {
    vi.mocked(notesImport).mockResolvedValue(
      { imported: 3, updated: 2, skipped: 1, errors: [] } as never,
    );
    const t = await setup();
    await pickFile(t, '{"notes":[{"id":"a"}]}');
    await confirmImport();

    expect(notesImport).toHaveBeenCalledWith('{"notes":[{"id":"a"}]}', "overwrite", undefined);
    expect(status(t)).toContain("3 new");
    expect(status(t)).toContain("2 updated");
    expect(status(t)).toContain("1 skipped");
  });

  it("says so when the backup had nothing in it", async () => {
    const t = await setup();
    await pickFile(t, '{"notes":[]}');
    await confirmImport();
    expect(status(t)).toContain("Nothing to import.");
  });

  it("counts errors in the summary", async () => {
    vi.mocked(notesImport).mockResolvedValue(
      { imported: 1, updated: 0, skipped: 0, errors: ["bad row"] } as never,
    );
    const t = await setup();
    await pickFile(t, '{"notes":[{"id":"a"}]}');
    await confirmImport();
    expect(status(t)).toContain("1 errors");
  });

  it("imports nothing when the confirm is cancelled", async () => {
    const t = await setup();
    await pickFile(t, '{"notes":[{"id":"a"}]}');
    document.querySelector<HTMLButtonElement>(".btn-cancel")!.click();
    await settle();

    expect(notesImport).not.toHaveBeenCalled();
    expect(document.querySelector(".modal")).toBeNull();
  });

  it("reports a failed import", async () => {
    vi.mocked(notesImport).mockRejectedValue(new Error("schema mismatch"));
    const t = await setup();
    await pickFile(t, '{"notes":[{"id":"a"}]}');
    await confirmImport();
    expect(status(t)).toContain("Import failed");
    expect(status(t)).toContain("schema mismatch");
  });
});

// A backup seals protected notes; importing one without the password would
// either fail late or quietly drop them.
describe("a backup containing protected notes", () => {
  const sealed = '{"notes":[{"id":"a","secret":"deadbeef"}]}';

  it("asks for the password rather than importing straight away", async () => {
    const t = await setup();
    await pickFile(t, sealed);
    await confirmImport();

    expect(notesImport).not.toHaveBeenCalled();
    expect(document.querySelector<HTMLInputElement>('.modal input[type="password"]')).toBeTruthy();
  });

  it("passes the password through to the import", async () => {
    const t = await setup();
    await pickFile(t, sealed);
    await confirmImport();

    const pw = document.querySelector<HTMLInputElement>('.modal input[type="password"]')!;
    pw.value = "hunter2";
    pw.dispatchEvent(new Event("input", { bubbles: true }));
    document.querySelector("form")!.dispatchEvent(
      new Event("submit", { bubbles: true, cancelable: true }),
    );
    await settle();

    expect(notesImport).toHaveBeenCalledWith(sealed, "overwrite", "hunter2");
  });

  it("does not treat an ordinary backup as sealed", async () => {
    const t = await setup();
    await pickFile(t, '{"notes":[{"id":"a","title":"plain"}]}');
    await confirmImport();
    expect(notesImport).toHaveBeenCalled();
  });

  it("does not treat unparseable contents as sealed", async () => {
    const t = await setup();
    await pickFile(t, "this is not json at all");
    await confirmImport();
    expect(notesImport).toHaveBeenCalledWith("this is not json at all", "overwrite", undefined);
  });
});

describe("device name", () => {
  const nameInput = (t: HTMLElement) => t.querySelector<HTMLInputElement>("input.name-input");

  async function startEditing(t: HTMLElement) {
    t.querySelector<HTMLButtonElement>("button.name-value")!.click();
    await settle();
  }

  it("saves a new name", async () => {
    const t = await setup();
    await startEditing(t);
    const input = nameInput(t)!;
    input.value = "Laptop";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settle();

    expect(setDeviceName).toHaveBeenCalledWith("Laptop");
    expect(status(t)).toContain("Laptop");
  });

  it("does not write an unchanged name", async () => {
    const t = await setup();
    await startEditing(t);
    const input = nameInput(t)!;
    input.value = "Workstation";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settle();
    expect(setDeviceName).not.toHaveBeenCalled();
  });

  it("does not write a blank name", async () => {
    const t = await setup();
    await startEditing(t);
    const input = nameInput(t)!;
    input.value = "   ";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settle();
    expect(setDeviceName).not.toHaveBeenCalled();
  });

  it("abandons the edit on Escape", async () => {
    const t = await setup();
    await startEditing(t);
    const input = nameInput(t)!;
    input.value = "Discarded";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await settle();

    expect(setDeviceName).not.toHaveBeenCalled();
    expect(status(t)).toContain("Workstation");
  });

  it("keeps the old name when the write fails", async () => {
    vi.mocked(setDeviceName).mockRejectedValue(new Error("keychain locked"));
    const t = await setup();
    await startEditing(t);
    const input = nameInput(t)!;
    input.value = "Laptop";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await settle();

    expect(status(t)).toContain("Workstation");
    expect(status(t)).not.toContain("keychain locked");
  });
});

describe("receiving notes", () => {
  const toggle = (t: HTMLElement) =>
    rowByTitle(t, /Receive notes/)!.querySelector<HTMLElement>("button, .switch, input")
      ?? rowByTitle(t, /Receive notes/)!;

  it("starts the listener when switched on", async () => {
    const t = await setup();
    toggle(t).click();
    await settle();
    expect(startReceiving).toHaveBeenCalled();
    expect(stopReceiving).not.toHaveBeenCalled();
  });

  it("stops it when switched off", async () => {
    vi.mocked(isReceiving).mockResolvedValue(true);
    const t = await setup();
    toggle(t).click();
    await settle();
    expect(stopReceiving).toHaveBeenCalled();
    expect(startReceiving).not.toHaveBeenCalled();
  });

  it("leaves the switch alone when the backend refuses", async () => {
    vi.mocked(startReceiving).mockRejectedValue(new Error("port in use"));
    const t = await setup();
    toggle(t).click();
    await settle();
    expect(status(t)).not.toContain("port in use");
  });
});
