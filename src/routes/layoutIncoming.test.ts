// @vitest-environment happy-dom
//
// A note delivered by the single-note protocol - what older senders still use -
// arrived, was stored encrypted, and then had nowhere to surface: the layout
// listened for `transfer-offer` and `notes-received` but not `transfer-received`,
// and never polled `pending_transfers_list`. The note sat unreachable in memory
// and was lost when the app closed.
//
// Found on a real Android device, not by the protocol tests, which call the
// accept path directly and so never touch this wiring.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import { writable } from "svelte/store";

const h = vi.hoisted(() => ({
  listeners: new Map<string, () => void>(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: () => void) => {
    h.listeners.set(name, cb);
    return () => h.listeners.delete(name);
  }),
}));

vi.mock("$lib/tauri", () => ({
  pendingOffersList: vi.fn(async () => []),
  pendingTransfersList: vi.fn(async () => []),
  isReceiving: vi.fn(async () => true),
  startReceiving: vi.fn(async () => {}),
  stopReceiving: vi.fn(async () => {}),
  transferOfferRespond: vi.fn(async () => {}),
  noteReceiveAccept: vi.fn(async () => "new-note-id"),
  noteReceiveReject: vi.fn(async () => {}),
}));

vi.mock("$lib/stores/notes", () => ({
  refreshNotes: vi.fn(async () => {}),
  notes: writable([]),
  bgImages: writable({}),
  totalNotes: writable(0),
}));

vi.mock("$lib/stores/theme", () => ({ initTheme: vi.fn(() => () => {}), theme: writable("candy-light") }));
vi.mock("$lib/stores/sidebar", () => ({ sidebarOpen: writable(false) }));
vi.mock("$lib/stores/layout", () => ({ isDesktop: writable(false) }));
vi.mock("$lib/stores/listState", () => ({
  listFilter: writable(""), listSelecting: writable(false),
  listSelected: writable(new Set()), resetListState: vi.fn(),
}));

// The layout mounts the list pane and the sidebar; neither is under test here.
vi.mock("$lib/components/NoteListPane.svelte", () => ({ default: () => ({}) }));
vi.mock("$lib/components/Sidebar.svelte", () => ({ default: () => ({}) }));
vi.mock("$lib/components/NewNoteModal.svelte", () => ({ default: () => ({}) }));

import { pendingTransfersList, noteReceiveAccept } from "$lib/tauri";
import { refreshNotes } from "$lib/stores/notes";
import Layout from "./+layout.svelte";

const flush = async () => { await Promise.resolve(); await new Promise(r => setTimeout(r, 0)); };
const settle = async () => { for (let i = 0; i < 6; i++) await flush(); };

let cleanup: (() => void) | null = null;

const pending = (id: string) => ({ transfer_id: id, from_peer: "Windows", received_at: 1_700_000_000 });

async function setup() {
  const target = document.createElement("div");
  document.body.appendChild(target);
  // The layout renders nothing outside Tauri.
  (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ = {};
  const app = mount(Layout, { target, props: { children: () => ({}) } as never });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  await settle();
  return target;
}

beforeEach(() => {
  vi.clearAllMocks();
  h.listeners.clear();
  vi.mocked(pendingTransfersList).mockResolvedValue([] as never);
  vi.mocked(noteReceiveAccept).mockResolvedValue("new-note-id" as never);
});

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
  delete (window as unknown as Record<string, unknown>).__TAURI_INTERNALS__;
});

describe("incoming single-note transfers", () => {
  it("listens for the event the backend actually emits", async () => {
    await setup();
    expect([...h.listeners.keys()]).toContain("transfer-received");
  });

  it("asks the backend what is waiting, not just for offers", async () => {
    await setup();
    expect(pendingTransfersList).toHaveBeenCalled();
  });

  it("surfaces a delivered note so the user can open it", async () => {
    vi.mocked(pendingTransfersList).mockResolvedValue([pending("t1")] as never);
    const target = await setup();
    expect(target.querySelector(".toast")).toBeTruthy();
    expect(target.textContent).toContain("Windows wants to send");
  });

  it("re-polls when the backend says one arrived", async () => {
    const target = await setup();
    expect(target.querySelector(".toast")).toBeNull();

    vi.mocked(pendingTransfersList).mockResolvedValue([pending("t1")] as never);
    h.listeners.get("transfer-received")!();
    await settle();

    expect(target.querySelector(".toast")).toBeTruthy();
  });

  // The note is imported by the accept call itself, with no event to announce
  // it, so without an explicit refresh it stays invisible until a restart.
  it("refreshes the note list once a delivered note is opened", async () => {
    vi.mocked(pendingTransfersList).mockResolvedValue([pending("t1")] as never);
    const target = await setup();

    const input = target.querySelector<HTMLInputElement>(".code-input")!;
    input.value = "K4X7P2";
    input.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();

    vi.mocked(refreshNotes).mockClear();
    target.querySelector<HTMLButtonElement>(".btn-accept")!.click();
    await settle();

    expect(noteReceiveAccept).toHaveBeenCalledWith("t1", "K4X7P2");
    expect(refreshNotes).toHaveBeenCalled();
  });
});
