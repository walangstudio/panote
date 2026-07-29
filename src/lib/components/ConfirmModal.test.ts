// @vitest-environment happy-dom
//
// The unsaved-changes prompt gained a third action (Save / Discard / Cancel).
// Losing a note's edits is the worst failure this app has, so each button is
// asserted to call its own handler and nothing else.
import { describe, it, expect, afterEach, vi } from "vitest";
import { mount, unmount, type ComponentProps } from "svelte";
import ConfirmModal from "./ConfirmModal.svelte";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

/// `withAlt` controls whether the `onalt` handler is passed, so we can cover the
/// case where a caller supplies a label but no handler.
type Props = ComponentProps<typeof ConfirmModal>;

function setup(extra: Partial<Props> = {}, withAlt = true) {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const onconfirm = vi.fn();
  const oncancel = vi.fn();
  const onalt = vi.fn();
  const props: Props = {
    title: "Unsaved changes",
    message: "Save?",
    onconfirm,
    oncancel,
    ...extra,
  };
  if (withAlt && extra.altLabel) props.onalt = onalt;
  const app = mount(ConfirmModal, { target, props });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target, onconfirm, oncancel, onalt };
}

const byText = (t: HTMLElement, label: string) =>
  [...t.querySelectorAll("button")].find(b => b.textContent?.trim() === label);

describe("ConfirmModal", () => {
  it("shows only cancel and confirm by default", () => {
    const { target } = setup({ confirmLabel: "Delete" });
    expect(target.querySelectorAll(".actions button").length).toBe(2);
    expect(target.querySelector(".btn-alt")).toBeNull();
  });

  it("renders the third action when both altLabel and onalt are given", () => {
    const { target } = setup({ confirmLabel: "Save", altLabel: "Discard" });
    expect(target.querySelectorAll(".actions button").length).toBe(3);
    expect(byText(target, "Discard")).toBeTruthy();
  });

  it("omits the third action when only altLabel is given", () => {
    const { target } = setup({ altLabel: "Discard" }, false);
    expect(target.querySelector(".btn-alt")).toBeNull();
  });

  it("routes each button to its own handler", () => {
    const { target, onconfirm, oncancel, onalt } = setup({
      confirmLabel: "Save", altLabel: "Discard",
    });

    byText(target, "Discard")!.click();
    expect(onalt).toHaveBeenCalledTimes(1);
    expect(onconfirm).not.toHaveBeenCalled();
    expect(oncancel).not.toHaveBeenCalled();

    byText(target, "Save")!.click();
    expect(onconfirm).toHaveBeenCalledTimes(1);
    expect(onalt).toHaveBeenCalledTimes(1);

    byText(target, "Cancel")!.click();
    expect(oncancel).toHaveBeenCalledTimes(1);
  });

  it("Escape cancels, and never reaches the destructive alt action", async () => {
    const { onconfirm, oncancel, onalt } = setup({
      confirmLabel: "Save", altLabel: "Discard",
    });
    await new Promise(r => setTimeout(r, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    expect(oncancel).toHaveBeenCalledTimes(1);
    expect(onalt).not.toHaveBeenCalled();
    expect(onconfirm).not.toHaveBeenCalled();
  });

  // The bug this replaces: a window-level Enter handler ran onconfirm no matter
  // what had focus, so a stray Enter on the delete dialog destroyed a note —
  // and Discard could never be reached by keyboard. Enter must belong to the
  // focused button, not to the window.
  it("a stray Enter does not confirm", async () => {
    const { onconfirm, onalt } = setup({ confirmLabel: "Delete", destructive: true });
    await new Promise(r => setTimeout(r, 0));

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }));
    expect(onconfirm).not.toHaveBeenCalled();
    expect(onalt).not.toHaveBeenCalled();
  });

  it("a destructive dialog focuses Cancel, so the reflex is harmless", async () => {
    const { target } = setup({ confirmLabel: "Delete", destructive: true });
    await new Promise(r => setTimeout(r, 0));
    expect(document.activeElement).toBe(byText(target, "Cancel"));
  });

  it("a non-destructive dialog focuses its primary action", async () => {
    const { target } = setup({ confirmLabel: "Save" });
    await new Promise(r => setTimeout(r, 0));
    expect(document.activeElement).toBe(byText(target, "Save"));
  });

  it("restores focus to whatever was focused before it opened", async () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    expect(document.activeElement).toBe(opener);

    setup({ confirmLabel: "Delete", destructive: true });
    await new Promise(r => setTimeout(r, 0));
    cleanup?.();
    cleanup = null;
    await new Promise(r => setTimeout(r, 0));
    expect(document.activeElement).toBe(opener);
  });
});
