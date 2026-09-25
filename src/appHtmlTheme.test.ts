// @vitest-environment happy-dom
//
// The first paint comes from an inline script in app.html; initTheme() only
// takes over after hydration. The script has to resolve a stored preference the
// same way the store does, so it runs here straight out of the file and a drift
// between the two fails this instead of showing up as a flash on launch.
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { readFileSync } from "fs";

const script = readFileSync("src/app.html", "utf-8").match(/<script>([\s\S]*?)<\/script>/)?.[1];

let osDark = false;

function run() {
  new Function(script!)();
  return document.documentElement.dataset.theme;
}

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  osDark = false;
  vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: osDark })));
});

afterEach(() => vi.unstubAllGlobals());

describe("app.html early theme paint", () => {
  it("ships an inline script", () => {
    expect(script).toBeTruthy();
  });

  it("applies a stored explicit theme whatever the OS says", () => {
    localStorage.setItem("panote-theme", "candy-light");
    osDark = true;
    expect(run()).toBe("candy-light");
    localStorage.setItem("panote-theme", "candy-dark");
    osDark = false;
    expect(run()).toBe("candy-dark");
  });

  it("resolves a stored system preference from the OS", () => {
    localStorage.setItem("panote-theme", "system");
    osDark = true;
    expect(run()).toBe("candy-dark");
    osDark = false;
    expect(run()).toBe("candy-light");
  });

  it("follows the OS on a fresh install", () => {
    osDark = true;
    expect(run()).toBe("candy-dark");
  });
});
