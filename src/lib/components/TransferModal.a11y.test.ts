// @vitest-environment happy-dom
//
// The wizard has 6 steps (peers/code/unlock/sending/done/error) but nothing
// announced the transition between them to a screen-reader user — each new
// heading just silently replaced the last one in the accessibility tree.
import { describe, it, expect, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";

const { peer } = vi.hoisted(() => ({
  peer: { id: "p1", name: "Peer One", address: "10.0.0.2", port: 1, via: "lan" as const },
}));

vi.mock("$lib/tauri", () => ({
  peersScan: vi.fn().mockResolvedValue([peer]),
  notesSend: vi.fn().mockResolvedValue(undefined),
  generatePairingCode: vi.fn().mockResolvedValue("ABC123"),
  knownPeersList: vi.fn().mockResolvedValue([]),
  peerAddManual: vi.fn(),
  deviceIps: vi.fn().mockResolvedValue([]),
  noteList: vi.fn().mockResolvedValue([]),
  noteUnlock: vi.fn(),
}));

import TransferModal from "./TransferModal.svelte";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

function setup() {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const onclose = vi.fn();
  const app = mount(TransferModal, { target, props: { noteIds: ["n1"], onclose } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target };
}

describe("TransferModal live region", () => {
  it("has a persistent status region that announces the current step", async () => {
    const { target } = setup();
    await new Promise(r => setTimeout(r, 0));

    const status = target.querySelector('[role="status"]');
    expect(status, "expected a role=status live region").toBeTruthy();
    expect(status?.getAttribute("aria-live")).toBe("polite");
    const onPeersStep = status?.textContent?.trim();
    expect(onPeersStep).toBeTruthy();

    const peerBtn = [...target.querySelectorAll(".peer-item")].find(b => b.textContent?.includes("Peer One")) as HTMLButtonElement;
    peerBtn.click();
    await new Promise(r => setTimeout(r, 0));
    const next = [...target.querySelectorAll("button")].find(b => b.textContent?.trim() === "Next") as HTMLButtonElement;
    next.click();
    await new Promise(r => setTimeout(r, 0));

    // Same element (not destroyed/recreated), new announcement.
    const statusAfter = target.querySelector('[role="status"]');
    expect(statusAfter).toBe(status);
    expect(statusAfter?.textContent?.trim()).not.toBe(onPeersStep);
  });
});
