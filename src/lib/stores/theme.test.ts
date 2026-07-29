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
let toggleDarkMode: typeof import("./theme")["toggleDarkMode"];
let stop: (() => void) | null = null;

// The store captures localStorage at module scope, so each test needs a fresh
// module instance rather than a fresh store value.
async function load() {
  vi.resetModules();
  const mod = await import("./theme");
  theme = mod.theme;
  initTheme = mod.initTheme;
  toggleDarkMode = mod.toggleDarkMode;
}

const read = () => {
  let v = "";
  theme.subscribe(x => (v = x))();
  return v;
};

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});

afterEach(() => {
  stop?.();
  stop = null;
});

describe("initial value", () => {
  it("falls back to candy-light with nothing stored", async () => {
    await load();
    expect(read()).toBe("candy-light");
  });

  it("prefers whatever localStorage already holds", async () => {
    localStorage.setItem(STORAGE_KEY, "candy-dark");
    await load();
    expect(read()).toBe("candy-dark");
  });
});

describe("initTheme", () => {
  it("paints the html element before the DB has answered", async () => {
    localStorage.setItem(STORAGE_KEY, "candy-dark");
    await load();
    stop = initTheme();
    // synchronously, no await — this is the anti-flash path
    expect(document.documentElement.dataset.theme).toBe("candy-dark");
  });

  it("adopts the DB value once it arrives", async () => {
    vi.mocked(getTheme).mockResolvedValue("candy-dark");
    await load();
    stop = initTheme();
    expect(read()).toBe("candy-light");
    await flush();
    expect(read()).toBe("candy-dark");
    expect(document.documentElement.dataset.theme).toBe("candy-dark");
  });

  it("ignores a DB value that is not a known theme", async () => {
    vi.mocked(getTheme).mockResolvedValue("neon-disco" as unknown as string);
    await load();
    stop = initTheme();
    await flush();
    expect(read()).toBe("candy-light");
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
    expect(document.documentElement.dataset.theme).toBe("candy-dark");
  });

  it("stops painting the document once unsubscribed", async () => {
    await load();
    const unsub = initTheme();
    await flush();
    unsub();
    theme.set("candy-dark");
    expect(document.documentElement.dataset.theme).toBe("candy-light");
  });
});

describe("toggleDarkMode", () => {
  it("goes light to dark and back", async () => {
    await load();
    stop = initTheme();
    await flush();

    toggleDarkMode();
    expect(read()).toBe("candy-dark");
    toggleDarkMode();
    expect(read()).toBe("candy-light");
  });

  it("treats any non-dark value as light, so one toggle always reaches dark", async () => {
    localStorage.setItem(STORAGE_KEY, "something-else");
    await load();
    toggleDarkMode();
    expect(read()).toBe("candy-dark");
  });
});
