// @vitest-environment happy-dom
//
// The toast is the receiving half of the LAN pairing handshake: the sender
// reads a short code aloud and the receiver types it. Per-offer state is kept
// in maps keyed by offer_id, so the risk is one offer's code, error or spinner
// bleeding into another's.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import type { PendingOffer } from "$lib/tauri";

vi.mock("$lib/tauri", () => ({ transferOfferRespond: vi.fn(async () => {}) }));

import { transferOfferRespond } from "$lib/tauri";
import IncomingTransferToast from "./IncomingTransferToast.svelte";

const offer = (id: string, over: Partial<PendingOffer> = {}): PendingOffer => ({
  offer_id: id,
  from_peer: "Laptop",
  note_count: 1,
  ...over,
}) as PendingOffer;

let cleanup: (() => void) | null = null;
const flush = async () => { await Promise.resolve(); await new Promise(r => setTimeout(r, 0)); };

function setup(offers: PendingOffer[]) {
  const onupdate = vi.fn();
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(IncomingTransferToast, { target, props: { offers, onupdate } });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target, onupdate };
}

const toasts = (t: HTMLElement) => [...t.querySelectorAll(".toast")];
const codeInput = (t: HTMLElement, i = 0) =>
  t.querySelectorAll<HTMLInputElement>(".code-input")[i];
const acceptBtn = (t: HTMLElement, i = 0) =>
  t.querySelectorAll<HTMLButtonElement>(".btn-accept")[i];
const dismissBtn = (t: HTMLElement, i = 0) =>
  t.querySelectorAll<HTMLButtonElement>(".btn-decline")[i];

async function typeCode(t: HTMLElement, code: string, i = 0) {
  const el = codeInput(t, i);
  el.value = code;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  await flush();
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(transferOfferRespond).mockResolvedValue(undefined as never);
});
afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

describe("rendering offers", () => {
  it("shows nothing at all when there are none", () => {
    const { target } = setup([]);
    expect(target.querySelector(".toast-stack")).toBeNull();
  });

  it("names the sender and the note count", () => {
    const { target } = setup([offer("o1", { from_peer: "Phone", note_count: 4 })]);
    expect(target.textContent).toContain("Phone wants to send");
    expect(target.textContent).toContain("4 notes");
  });

  it("uses the singular for a single note", () => {
    const { target } = setup([offer("o1", { note_count: 1 })]);
    expect(target.textContent).toContain("1 note");
    expect(target.textContent).not.toContain("1 notes");
  });

  it("stacks concurrent offers", () => {
    const { target } = setup([offer("o1"), offer("o2"), offer("o3")]);
    expect(toasts(target)).toHaveLength(3);
  });
});

describe("accepting", () => {
  it("sends the typed code for that offer", async () => {
    const { target, onupdate } = setup([offer("o1")]);
    await typeCode(target, "K4X7P2");
    acceptBtn(target).click();
    await flush();

    expect(transferOfferRespond).toHaveBeenCalledWith("o1", "K4X7P2");
    expect(onupdate).toHaveBeenCalled();
  });

  // The placeholder shows a dashed code, so the dashes have to be tolerated.
  it("strips the dashes the sender reads out", async () => {
    const { target } = setup([offer("o1")]);
    await typeCode(target, "k4x-7p2");
    acceptBtn(target).click();
    await flush();
    expect(transferOfferRespond).toHaveBeenCalledWith("o1", "K4X7P2");
  });

  it("accepts on Enter as well as the button", async () => {
    const { target } = setup([offer("o1")]);
    await typeCode(target, "AAA111");
    codeInput(target).dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    await flush();
    expect(transferOfferRespond).toHaveBeenCalledWith("o1", "AAA111");
  });

  it("refuses an empty code instead of calling the backend", async () => {
    const { target, onupdate } = setup([offer("o1")]);
    acceptBtn(target).click();
    await flush();

    expect(transferOfferRespond).not.toHaveBeenCalled();
    expect(onupdate).not.toHaveBeenCalled();
    expect(target.textContent).toContain("Enter the code from the sender.");
  });

  it("shows a rejected code and keeps the toast up to retry", async () => {
    vi.mocked(transferOfferRespond).mockRejectedValue("WRONG_CODE");
    const { target, onupdate } = setup([offer("o1")]);
    await typeCode(target, "BADBAD");
    acceptBtn(target).click();
    await flush();

    expect(target.textContent).toContain("WRONG_CODE");
    expect(onupdate).not.toHaveBeenCalled();
    expect(toasts(target)).toHaveLength(1);
    expect(acceptBtn(target).disabled).toBe(false);
  });

  it("clears the previous error on the next attempt", async () => {
    const { target } = setup([offer("o1")]);
    acceptBtn(target).click();
    await flush();
    expect(target.textContent).toContain("Enter the code");

    await typeCode(target, "GOOD11");
    acceptBtn(target).click();
    await flush();
    expect(target.textContent).not.toContain("Enter the code");
  });

  it("disables both buttons while the handshake is in flight", async () => {
    let release: (() => void) | null = null;
    vi.mocked(transferOfferRespond).mockImplementation(
      () => new Promise<void>(r => { release = r; }) as never,
    );
    const { target } = setup([offer("o1")]);
    await typeCode(target, "AAA111");
    acceptBtn(target).click();
    await flush();

    expect(acceptBtn(target).disabled).toBe(true);
    expect(dismissBtn(target).disabled).toBe(true);
    expect(acceptBtn(target).textContent).toContain("Accepting");

    release!();
    await flush();
    expect(acceptBtn(target).disabled).toBe(false);
  });
});

describe("keeping offers apart", () => {
  it("sends each offer its own code", async () => {
    const { target } = setup([offer("o1"), offer("o2")]);
    await typeCode(target, "FIRST1", 0);
    await typeCode(target, "SECON2", 1);

    acceptBtn(target, 1).click();
    await flush();
    expect(transferOfferRespond).toHaveBeenCalledWith("o2", "SECON2");

    acceptBtn(target, 0).click();
    await flush();
    expect(transferOfferRespond).toHaveBeenCalledWith("o1", "FIRST1");
  });

  it("shows an error only on the offer that failed", async () => {
    vi.mocked(transferOfferRespond).mockRejectedValue("WRONG_CODE");
    const { target } = setup([offer("o1"), offer("o2")]);
    await typeCode(target, "BADBAD", 0);
    acceptBtn(target, 0).click();
    await flush();

    expect(toasts(target)[0].textContent).toContain("WRONG_CODE");
    expect(toasts(target)[1].textContent).not.toContain("WRONG_CODE");
  });

  it("only spins the offer being accepted", async () => {
    vi.mocked(transferOfferRespond).mockImplementation(() => new Promise<void>(() => {}) as never);
    const { target } = setup([offer("o1"), offer("o2")]);
    await typeCode(target, "AAA111", 0);
    acceptBtn(target, 0).click();
    await flush();

    expect(acceptBtn(target, 0).disabled).toBe(true);
    expect(acceptBtn(target, 1).disabled).toBe(false);
  });
});

describe("dismissing", () => {
  it("hides that offer and lets the sender time out", async () => {
    const { target } = setup([offer("o1")]);
    dismissBtn(target).click();
    await flush();

    expect(target.querySelector(".toast-stack")).toBeNull();
    expect(transferOfferRespond).not.toHaveBeenCalled();
  });

  it("leaves the other offers showing", async () => {
    const { target } = setup([offer("o1"), offer("o2", { from_peer: "Tablet" })]);
    dismissBtn(target, 0).click();
    await flush();

    expect(toasts(target)).toHaveLength(1);
    expect(target.textContent).toContain("Tablet");
  });
});
