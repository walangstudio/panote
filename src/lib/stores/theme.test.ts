// @vitest-environment happy-dom
//
// Theme is persisted twice on purpose: localStorage paints instantly on the next
// launch, the DB survives the webview dropping localStorage. Both writes have to
// keep happening, and neither may throw far enough to break the app.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";

vi.mock("$lib/tauri", () => ({
  getTheme: vi.fn(async () => "candy-light"),
  setTheme: vi.fn(async () => {}),
}));

import { getTheme, setTheme } from "$lib/tauri";

const STORAGE_KEY = "panote-theme";
const flush = () => new Promise(r => setTimeout(r, 0));

let theme: typeof import("./theme")["theme"];
let initTheme: typeof import("./theme")["initTheme"];
let stop: (() => void) | null = null;

// Same reason as layout.test.ts: a fresh module per test means resetModules plus
// a dynamic import, and that recompile can pass the 5s default while the whole
// suite runs in parallel. It only ever failed under load, and intermittently,
// which is what made it hard to pin down. The work is slow, not broken.
vi.setConfig({ testTimeout: 20_000 });

// The store captures localStorage at module scope, so each test needs a fresh
// module instance rather than a fresh store value.
async function load() {
  vi.resetModules();
  const mod = await import("./theme");
  theme = mod.theme;
  initTheme = mod.initTheme;
}

const read = () => {
  let v = "";
  theme.subscribe(x => (v = x))();
  return v;
};
const painted = () => document.documentElement.dataset.theme;

// happy-dom never matches prefers-color-scheme, so the OS preference is stubbed
// and flipped by hand.
type Listener = (e: { matches: boolean }) => void;
let osDark = false;
let listeners = new Set<Listener>();
function osChange(dark: boolean) {
  osDark = dark;
  for (const fn of listeners) fn({ matches: dark });
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  osDark = false;
  listeners = new Set();
  vi.stubGlobal("matchMedia", vi.fn(() => ({
    get matches() { return osDark; },
    addEventListener: (_: string, fn: Listener) => listeners.add(fn),
    removeEventListener: (_: string, fn: Listener) => listeners.delete(fn),
  })));
});

afterEach(() => {
  stop?.();
  stop = null;
  vi.unstubAllGlobals();
});

describe("initial value", () => {
  it("follows the system with nothing stored", async () => {
    await load();
    expect(read()).toBe("system");
  });

  it("prefers whatever localStorage already holds", async () => {
    localStorage.setItem(STORAGE_KEY, "candy-dark");
    await load();
    expect(read()).toBe("candy-dark");
  });

  it("ignores a stored value that is not a known preference", async () => {
    localStorage.setItem(STORAGE_KEY, "something-else");
    await load();
    expect(read()).toBe("system");
  });
});

describe("initTheme", () => {
  it("paints the html element before the DB has answered", async () => {
    localStorage.setItem(STORAGE_KEY, "candy-dark");
    await load();
    stop = initTheme();
    // synchronously, no await: this is the anti-flash path
    expect(painted()).toBe("candy-dark");
  });

  it("adopts the DB value once it arrives", async () => {
    vi.mocked(getTheme).mockResolvedValue("candy-dark");
    await load();
    stop = initTheme();
    expect(read()).toBe("system");
    await flush();
    expect(read()).toBe("candy-dark");
    expect(painted()).toBe("candy-dark");
  });

  it("adopts a system preference from the DB", async () => {
    vi.mocked(getTheme).mockResolvedValue("system");
    localStorage.setItem(STORAGE_KEY, "candy-light");
    await load();
    stop = initTheme();
    await flush();
    expect(read()).toBe("system");
  });

  it("ignores a DB value that is not a known preference", async () => {
    vi.mocked(getTheme).mockResolvedValue("neon-disco" as unknown as string);
    await load();
    stop = initTheme();
    await flush();
    expect(read()).toBe("system");
  });

  it("survives the DB read failing", async () => {
    vi.mocked(getTheme).mockRejectedValue(new Error("db down"));
    localStorage.setItem(STORAGE_KEY, "candy-dark");
    await load();
    stop = initTheme();
    await flush();
    expect(read()).toBe("candy-dark");
  });

  // The `ready` flag exists so the initial read does not immediately echo back
  // as a write, which would race the DB fetch it just resolved.
  it("does not write back to the DB before the initial read resolves", async () => {
    await load();
    stop = initTheme();
    expect(setTheme).not.toHaveBeenCalled();
  });

  it("persists later changes to both localStorage and the DB", async () => {
    await load();
    stop = initTheme();
    await flush();
    vi.mocked(setTheme).mockClear();

    theme.set("candy-dark");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("candy-dark");
    expect(setTheme).toHaveBeenCalledWith("candy-dark");
  });

  it("keeps working when the DB write rejects", async () => {
    vi.mocked(setTheme).mockRejectedValue(new Error("write failed"));
    await load();
    stop = initTheme();
    await flush();
    expect(() => theme.set("candy-dark")).not.toThrow();
    expect(painted()).toBe("candy-dark");
  });

  it("stops painting the document once unsubscribed", async () => {
    await load();
    const unsub = initTheme();
    await flush();
    unsub();
    theme.set("candy-dark");
    expect(painted()).toBe("candy-light");
  });
});

describe("system mode", () => {
  it("paints dark when the OS prefers dark", async () => {
    osDark = true;
    await load();
    stop = initTheme();
    expect(painted()).toBe("candy-dark");
  });

  it("paints light when the OS prefers light", async () => {
    await load();
    stop = initTheme();
    expect(painted()).toBe("candy-light");
  });

  // The preference stays "system"; only the painted theme moves.
  it("follows the OS when it changes and keeps the preference as system", async () => {
    await load();
    stop = initTheme();
    await flush();
    vi.mocked(setTheme).mockClear();

    osChange(true);
    expect(painted()).toBe("candy-dark");
    expect(read()).toBe("system");
    expect(localStorage.getItem(STORAGE_KEY)).toBe("system");
    expect(setTheme).not.toHaveBeenCalled();

    osChange(false);
    expect(painted()).toBe("candy-light");
  });

  it("ignores the OS once an explicit theme is chosen", async () => {
    osDark = true;
    await load();
    stop = initTheme();
    theme.set("candy-light");
    expect(painted()).toBe("candy-light");
    osChange(false);
    osChange(true);
    expect(painted()).toBe("candy-light");
  });

  it("goes back to following the OS when system is chosen again", async () => {
    localStorage.setItem(STORAGE_KEY, "candy-light");
    osDark = true;
    await load();
    stop = initTheme();
    expect(painted()).toBe("candy-light");
    theme.set("system");
    expect(painted()).toBe("candy-dark");
  });

  it("stops listening to the OS once unsubscribed", async () => {
    await load();
    const unsub = initTheme();
    unsub();
    expect(listeners.size).toBe(0);
    osChange(true);
    expect(painted()).toBe("candy-light");
  });
});
