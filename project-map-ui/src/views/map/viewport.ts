import {
  MAP_NODE_HEIGHT,
  MAP_NODE_WIDTH,
} from "../../layout/mapLayout";
import type { ViewportTransform } from "../../state/session";

export const NARROW_MAP_MAX_WIDTH = 760;
export const NARROW_MAP_READABLE_SCALE = 0.58;

export interface ViewportSurface {
  width: number;
  height: number;
}

export interface ViewportTarget {
  x: number;
  y: number;
}

export function shouldRecoverNarrowMapViewport(
  viewport: ViewportTransform,
  surface: ViewportSurface,
  target: ViewportTarget,
): boolean {
  if (
    surface.width <= 0 ||
    surface.height <= 0 ||
    surface.width > NARROW_MAP_MAX_WIDTH
  ) {
    return false;
  }
  if (viewport.scale < NARROW_MAP_READABLE_SCALE) {
    return true;
  }

  const padding = 8;
  const left = viewport.x + target.x * viewport.scale;
  const top = viewport.y + target.y * viewport.scale;
  const right = left + MAP_NODE_WIDTH * viewport.scale;
  const bottom = top + MAP_NODE_HEIGHT * viewport.scale;
  return (
    left < padding ||
    right > surface.width - padding ||
    top < padding ||
    bottom > surface.height - padding
  );
}

export function viewportForMapTarget(
  viewport: ViewportTransform,
  surface: ViewportSurface,
  target: ViewportTarget,
  useNarrowReadableScale: boolean,
): ViewportTransform {
  const scale =
    useNarrowReadableScale && surface.width <= NARROW_MAP_MAX_WIDTH
      ? NARROW_MAP_READABLE_SCALE
      : viewport.scale;
  return {
    x: surface.width * 0.34 - (target.x + MAP_NODE_WIDTH / 2) * scale,
    y: surface.height * 0.46 - (target.y + MAP_NODE_HEIGHT / 2) * scale,
    scale,
  };
}
