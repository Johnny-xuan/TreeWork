import { describe, expect, it } from "vitest";
import {
  NARROW_MAP_READABLE_SCALE,
  shouldRecoverNarrowMapViewport,
  viewportForMapTarget,
} from "./viewport";

const surface = { width: 390, height: 354 };
const target = { x: 648, y: 120 };

describe("Map narrow viewport recovery", () => {
  it("recovers a desktop fit scale on a narrow canvas", () => {
    expect(
      shouldRecoverNarrowMapViewport(
        { x: -120, y: 20, scale: 0.42 },
        surface,
        target,
      ),
    ).toBe(true);
  });

  it("recovers a readable viewport when its target is outside the canvas", () => {
    expect(
      shouldRecoverNarrowMapViewport(
        { x: 40, y: 20, scale: 0.72 },
        surface,
        target,
      ),
    ).toBe(true);
  });

  it("preserves a readable viewport whose target is fully visible", () => {
    const located = viewportForMapTarget(
      { x: 0, y: 0, scale: 1.2 },
      surface,
      target,
      true,
    );
    expect(located.scale).toBe(NARROW_MAP_READABLE_SCALE);
    expect(
      shouldRecoverNarrowMapViewport(located, surface, target),
    ).toBe(false);
  });

  it("does not apply narrow recovery on desktop", () => {
    expect(
      shouldRecoverNarrowMapViewport(
        { x: -9999, y: -9999, scale: 0.42 },
        { width: 1440, height: 800 },
        target,
      ),
    ).toBe(false);
  });
});
