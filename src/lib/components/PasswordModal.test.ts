// @vitest-environment happy-dom
//
// Same class of bug as ConfirmModal: nothing moved focus into the dialog, and
// a stray Enter (via a window-level handler) submitted regardless of what had
// focus — so tabbing to Cancel and pressing Enter still submitted the form.
import { describe, it, expect, afterEach, vi } from "vitest";
import { mount, unmount, type ComponentProps } from "svelte";
import PasswordModal from "./PasswordModal.svelte";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
});

type Props = ComponentProps<typeof PasswordModal>;

function setup(extra: Partial<Props> = {}) {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const onsubmit = vi.fn().mockResolvedValue(undefined);
  const onclose = vi.fn();
  const props: Props = { mode: "unlock", onsubmit, onclose, ...extra };
  const app = mount(PasswordModal, { target, props });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target, onsubmit, onclose };
}

describe("PasswordModal", () => {
  it("focuses the first relevant field on mount", async () => {
    const { target } = setup({ mode: "set" });
    await new Promise(r => setTimeout(r, 0));
    const input = target.querySelector('input[placeholder="Password"]');
    expect(document.activeElement).toBe(input);
  });

  it("restores focus to whatever was focused before it opened", async () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();

    setup({ mode: "unlock" });
    await new Promise(r => setTimeout(r, 0));
    cleanup?.();
    cleanup = null;
    await new Promise(r => setTimeout(r, 0));
    expect(document.activeElement).toBe(opener);
  });

  it("a stray Enter while Cancel has focus cancels, not submits", async () => {
    const { target, onsubmit, onclose } = setup({ mode: "unlock" });
    await new Promise(r => setTimeout(r, 0));

    const cancelBtn = [...target.querySelectorAll("button")].find(b => b.textContent?.trim() === "Cancel")!;
    cancelBtn.focus();
    cancelBtn.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }));
    cancelBtn.click();

    expect(onclose).toHaveBeenCalledTimes(1);
    expect(onsubmit).not.toHaveBeenCalled();
  });
});
