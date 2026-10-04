// @vitest-environment happy-dom
//
// Nothing moved focus into this dialog on open, nor restored it on close.
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
  noteList: vi.fn().mockResolvedValue([
    { id: "n1", title: "Secret note", has_note_password: true },
  ]),
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
  return { target, onclose };
}

describe("TransferModal", () => {
  it("focuses the close button on mount", async () => {
    const { target } = setup();
    await new Promise(r => setTimeout(r, 0));
    const close = target.querySelector(".close");
    expect(document.activeElement).toBe(close);
  });

  it("closes on Escape", async () => {
    const { onclose } = setup();
    await new Promise(r => setTimeout(r, 0));
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(onclose).toHaveBeenCalledTimes(1);
  });

  it("restores focus to whatever was focused before it opened", async () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();

    setup();
    await new Promise(r => setTimeout(r, 0));
    cleanup?.();
    cleanup = null;
    await new Promise(r => setTimeout(r, 0));
    expect(document.activeElement).toBe(opener);
  });

  it("focuses the password field on reaching the unlock step (no more autofocus)", async () => {
    const { target } = setup();
    await new Promise(r => setTimeout(r, 0));

    const peerBtn = [...target.querySelectorAll(".peer-item")].find(b => b.textContent?.includes("Peer One")) as HTMLButtonElement;
    peerBtn.click();
    await new Promise(r => setTimeout(r, 0));

    const next = [...target.querySelectorAll("button")].find(b => b.textContent?.trim() === "Next") as HTMLButtonElement;
    next.click();
    await new Promise(r => setTimeout(r, 0));

    const send = [...target.querySelectorAll("button")].find(b => b.textContent?.trim() === "Send to peer") as HTMLButtonElement;
    send.click();
    await new Promise(r => setTimeout(r, 0));

    const unlockInput = target.querySelector('input[placeholder="Note password"]');
    expect(unlockInput).toBeTruthy();
    expect(unlockInput?.hasAttribute("autofocus")).toBe(false);
    expect(document.activeElement).toBe(unlockInput);
  });
});
