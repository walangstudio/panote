// @vitest-environment happy-dom
//
// The drawer stayed in the DOM at all times and was only slid off-screen with
// a CSS transform when closed. A transform doesn't remove an element from the
// tab order, so every page forced keyboard users through 5 invisible controls
// (New Note, Notes, Settings, Receiving, Theme) before reaching visible
// content. WCAG 2.4.3 (Focus Order) / 2.4.7 (Focus Visible).
import { describe, it, expect, afterEach, vi } from "vitest";
import { mount, unmount } from "svelte";
import Sidebar from "./Sidebar.svelte";
import { sidebarOpen } from "$lib/stores/sidebar";

let cleanup: (() => void) | null = null;

afterEach(() => {
  cleanup?.();
  cleanup = null;
  document.body.innerHTML = "";
  sidebarOpen.set(false);
});

function setup() {
  const target = document.createElement("div");
  document.body.appendChild(target);
  const app = mount(Sidebar, {
    target,
    props: { receiving: false, ontogglereceive: vi.fn(), onnewnote: vi.fn() },
  });
  cleanup = () => { try { unmount(app); } catch { /* teardown races are noise */ } };
  return { target };
}

describe("Sidebar", () => {
  it("is inert while closed, so its controls can't be tabbed to", () => {
    sidebarOpen.set(false);
    const { target } = setup();
    const drawer = target.querySelector("aside.drawer")!;
    expect(drawer.hasAttribute("inert")).toBe(true);
  });

  it("is interactive once opened", () => {
    sidebarOpen.set(true);
    const { target } = setup();
    const drawer = target.querySelector("aside.drawer")!;
    expect(drawer.hasAttribute("inert")).toBe(false);
  });
});
