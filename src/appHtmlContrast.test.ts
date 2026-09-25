// --muted was used for dates, previews, placeholders and empty states at
// 0.7-0.9rem — never large enough for the WCAG large-text exemption — while
// only hitting ~3.4-3.9:1 against --surface / --surface-container in both
// themes. Required: 4.5:1. This reads the shipped tokens straight out of
// app.html so a future edit that regresses either theme fails here too.
import { describe, it, expect } from "vitest";
import { readFileSync } from "fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

function hexToRgb(hex: string) {
  const h = hex.replace("#", "");
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
}
function luminance([r, g, b]: number[]) {
  const [R, G, B] = [r, g, b].map((v) => {
    v /= 255;
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * R + 0.7152 * G + 0.0722 * B;
}
function contrast(hex1: string, hex2: string) {
  const L1 = luminance(hexToRgb(hex1));
  const L2 = luminance(hexToRgb(hex2));
  const [hi, lo] = L1 > L2 ? [L1, L2] : [L2, L1];
  return (hi + 0.05) / (lo + 0.05);
}

function themeToken(src: string, theme: string, token: string): string {
  const block = src.match(new RegExp(`\\[data-theme="${theme}"\\]\\s*\\{([^}]*)\\}`))?.[1] ?? "";
  const value = block.match(new RegExp(`--${token}:\\s*(#[0-9a-fA-F]{6})`))?.[1];
  if (!value) throw new Error(`--${token} not found in [data-theme="${theme}"]`);
  return value;
}

describe("app.html --muted contrast (WCAG AA, 4.5:1 for normal text)", () => {
  const src = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "app.html"), "utf-8");

  for (const theme of ["candy-light", "candy-dark"] as const) {
    const muted = themeToken(src, theme, "muted");
    const surface = themeToken(src, theme, "surface");
    const surfaceContainer = themeToken(src, theme, "surface-container");

    it(`${theme}: --muted (${muted}) passes 4.5:1 against --surface (${surface})`, () => {
      expect(contrast(muted, surface)).toBeGreaterThanOrEqual(4.5);
    });

    it(`${theme}: --muted (${muted}) passes 4.5:1 against --surface-container (${surfaceContainer})`, () => {
      expect(contrast(muted, surfaceContainer)).toBeGreaterThanOrEqual(4.5);
    });
  }
});
