import { describe, it, expect } from "vitest";
import { placeMenu } from "./contextMenu";

const viewport = { width: 400, height: 600 };
const size = { width: 160, height: 200 };
const point = (x: number, y: number) => ({ left: x, right: x, top: y, bottom: y });

describe("placeMenu", () => {
  it("opens below and to the right of the anchor when there is room", () => {
    expect(placeMenu(point(50, 100), size, viewport)).toEqual({ left: 50, top: 100 });
  });

  it("opens upward when there is no room below", () => {
    expect(placeMenu(point(50, 500), size, viewport)).toEqual({ left: 50, top: 300 });
  });

  it("ends at the anchor's right edge when there is no room on the right", () => {
    const kebab = { left: 360, right: 390, top: 100, bottom: 130 };
    expect(placeMenu(kebab, size, viewport)).toEqual({ left: 230, top: 130 });
  });

  it("stays inside the viewport whatever the anchor", () => {
    for (const anchor of [point(0, 0), point(399, 599), point(-50, 700), { left: 10, right: 20, top: 150, bottom: 450 }]) {
      const { left, top } = placeMenu(anchor, size, viewport);
      expect(left).toBeGreaterThanOrEqual(8);
      expect(top).toBeGreaterThanOrEqual(8);
      expect(left + size.width).toBeLessThanOrEqual(viewport.width - 8);
      expect(top + size.height).toBeLessThanOrEqual(viewport.height - 8);
    }
  });
});
