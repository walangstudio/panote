// @vitest-environment happy-dom
//
// None of the modals actually trap Tab yet — PasswordModal/NewNoteModal/
// TransferModal/TableImportModal move focus in on mount and restore it on
// close, but Tab still walks straight out into the page behind the dialog.
import { describe, it, expect, afterEach } from "vitest";
import { trapFocus } from "./trapFocus";

let container: HTMLElement | null = null;
let destroy: (() => void) | null = null;

afterEach(() => {
  destroy?.();
  destroy = null;
  container?.remove();
  container = null;
});

function setup() {
  container = document.createElement("div");
  document.body.innerHTML = "";
  document.body.appendChild(container);
  container.innerHTML = `
    <button id="outside-before">outside before</button>
    <div id="modal">
      <button id="first">first</button>
      <button id="middle">middle</button>
      <button id="last">last</button>
    </div>
    <button id="outside-after">outside after</button>
  `;
  const modal = container.querySelector<HTMLElement>("#modal")!;
  const action = trapFocus(modal);
  destroy = () => action?.destroy?.();
  return {
    modal,
    first: container.querySelector<HTMLElement>("#first")!,
    last: container.querySelector<HTMLElement>("#last")!,
  };
}

function tab(shiftKey = false) {
  const e = new KeyboardEvent("keydown", { key: "Tab", shiftKey, bubbles: true, cancelable: true });
  document.activeElement?.dispatchEvent(e);
  return e;
}

describe("trapFocus", () => {
  it("wraps Tab from the last focusable element back to the first", () => {
    const { last, first } = setup();
    last.focus();
    const e = tab();
    expect(e.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(first);
  });

  it("wraps Shift+Tab from the first focusable element back to the last", () => {
    const { first, last } = setup();
    first.focus();
    const e = tab(true);
    expect(e.defaultPrevented).toBe(true);
    expect(document.activeElement).toBe(last);
  });

  it("leaves Tab alone away from the boundary elements", () => {
    const { modal } = setup();
    const middle = modal.querySelector<HTMLElement>("#middle")!;
    middle.focus();
    const e = tab();
    expect(e.defaultPrevented).toBe(false);
  });

  it("stops trapping once destroyed", () => {
    const { last, first } = setup();
    destroy?.();
    destroy = null;
    last.focus();
    const e = tab();
    expect(e.defaultPrevented).toBe(false);
    expect(document.activeElement).toBe(last);
    void first;
  });
});
