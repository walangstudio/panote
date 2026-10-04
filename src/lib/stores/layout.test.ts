// @vitest-environment happy-dom
//
// The split view is chosen by window width, not platform — a narrow desktop
// window gets the touch layout. The store has to track live resizes, because
// the list is route-mounted below the breakpoint and mounted in a pane above it.
import { describe, it, expect, beforeEach, vi } from "vitest";

const QUERY = "(min-width: 900px)";

type Listener = () => void;

let matches = true;
let listeners: Listener[] = [];
let removed: Listener[] = [];

function installMatchMedia() {
  listeners = [];
  removed = [];
  vi.stubGlobal("matchMedia", (q: string) => ({
    media: q,
    get matches() { return matches; },
    addEventListener: (_: string, fn: Listener) => listeners.push(fn),
    removeEventListener: (_: string, fn: Listener) => removed.push(fn),
  }));
}

// The store reads matchMedia once at module scope, so testing the initial value
// needs a genuinely fresh module — which means resetModules plus a dynamic import
// per test. That recompile can pass the 5s default while the whole suite is
// running in parallel, which made this file fail intermittently and only ever
// under load. The work is slow, not broken, so it gets longer to do it.
vi.setConfig({ testTimeout: 20_000 });

async function load() {
  vi.resetModules();
  return (await import("./layout")).isDesktop;
}

const resizeTo = (wide: boolean) => {
  matches = wide;
  listeners.forEach(fn => fn());
};

beforeEach(() => {
  matches = true;
  installMatchMedia();
});

describe("isDesktop", () => {
  it("starts true on a wide window", async () => {
    const isDesktop = await load();
    let v: boolean | undefined;
    isDesktop.subscribe(x => (v = x))();
    expect(v).toBe(true);
  });

  it("starts false on a narrow window", async () => {
    matches = false;
    const isDesktop = await load();
    let v: boolean | undefined;
    isDesktop.subscribe(x => (v = x))();
    expect(v).toBe(false);
  });

  it("queries the 900px breakpoint", async () => {
    const spy = vi.fn((q: string) => ({
      media: q, matches: true,
      addEventListener: () => {}, removeEventListener: () => {},
    }));
    vi.stubGlobal("matchMedia", spy);
    await load();
    expect(spy).toHaveBeenCalledWith(QUERY);
  });

  // The bug this guards: a store that only reads once leaves the app in the
  // wrong layout after the user drags the window narrower.
  it("follows the window across the breakpoint", async () => {
    const isDesktop = await load();
    const seen: boolean[] = [];
    const unsub = isDesktop.subscribe(v => seen.push(v));

    resizeTo(false);
    resizeTo(true);
    unsub();

    expect(seen).toEqual([true, false, true]);
  });

  it("detaches its listener when the last subscriber leaves", async () => {
    const isDesktop = await load();
    const unsub = isDesktop.subscribe(() => {});
    expect(listeners).toHaveLength(1);
    unsub();
    expect(removed).toHaveLength(1);
    expect(removed[0]).toBe(listeners[0]);
  });
});
