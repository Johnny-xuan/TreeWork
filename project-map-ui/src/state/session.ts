import type { LifecycleStatus, ReplaySpeed } from "../data/types";

export type ProjectMapView = "map" | "dependency" | "replay";

export interface ViewportTransform {
  x: number;
  y: number;
  scale: number;
}

export interface CanvasSettings {
  wheelMode: "pan" | "zoom";
  panSensitivity: number;
  zoomSensitivity: number;
}

export interface ProjectMapSession {
  activeView: ProjectMapView;
  selected: string;
  inspectorOpen: boolean;
  collapsed: string[];
  statusFilter: LifecycleStatus | "all";
  viewport: ViewportTransform | null;
  dependencyViewport: ViewportTransform | null;
  dependencyUpstreamDepth: number;
  dependencyDownstreamDepth: number;
  replaySelectedSeq: number | null;
  replayFollowLive: boolean;
  replayBranchFilter: string;
  replaySpeed: ReplaySpeed;
  replayViewport: ViewportTransform | null;
  replayCollapsed: string[];
  annotations: Record<string, string>;
  settings: CanvasSettings;
}

export const DEFAULT_CANVAS_SETTINGS: CanvasSettings = {
  wheelMode: "pan",
  panSensitivity: 0.55,
  zoomSensitivity: 0.35,
};

export const DEFAULT_SESSION: ProjectMapSession = {
  activeView: "map",
  selected: "",
  inspectorOpen: false,
  collapsed: [],
  statusFilter: "all",
  viewport: null,
  dependencyViewport: null,
  dependencyUpstreamDepth: 1,
  dependencyDownstreamDepth: 1,
  replaySelectedSeq: null,
  replayFollowLive: true,
  replayBranchFilter: "",
  replaySpeed: 1,
  replayViewport: null,
  replayCollapsed: [],
  annotations: {},
  settings: DEFAULT_CANVAS_SETTINGS,
};

const STORAGE_NAMESPACE = "treework-project-map:v3";

export function sessionKey(pathname = window.location.pathname): string {
  return `${STORAGE_NAMESPACE}:${pathname}`;
}

function finiteNumber(value: unknown, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value)
    ? value
    : fallback;
}

function viewportValue(value: unknown): ViewportTransform | null {
  if (
    !value ||
    typeof value !== "object" ||
    !("x" in value) ||
    !("y" in value) ||
    !("scale" in value) ||
    !Number.isFinite(value.x) ||
    !Number.isFinite(value.y) ||
    !Number.isFinite(value.scale)
  ) {
    return null;
  }
  return {
    x: value.x as number,
    y: value.y as number,
    scale: value.scale as number,
  };
}

function depthValue(value: unknown): number {
  return Math.min(99, Math.max(1, Math.floor(finiteNumber(value, 1))));
}

function replaySequenceValue(value: unknown): number | null {
  return typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value > 0
    ? value
    : null;
}

function replaySpeedValue(value: unknown): ReplaySpeed {
  return value === 0.5 || value === 2 || value === 4 ? value : 1;
}

export function readSession(): ProjectMapSession {
  try {
    const value = JSON.parse(
      window.sessionStorage.getItem(sessionKey()) ?? "null",
    ) as Partial<ProjectMapSession> | null;
    if (!value) {
      return structuredClone(DEFAULT_SESSION);
    }
    const settings = value.settings ?? DEFAULT_CANVAS_SETTINGS;
    const selected =
      typeof value.selected === "string" ? value.selected : "";
    return {
      activeView:
        value.activeView === "dependency" || value.activeView === "replay"
          ? value.activeView
          : "map",
      selected,
      inspectorOpen:
        typeof value.inspectorOpen === "boolean"
          ? value.inspectorOpen
          : Boolean(selected),
      collapsed: Array.isArray(value.collapsed)
        ? value.collapsed.filter(
            (item): item is string => typeof item === "string",
          )
        : [],
      statusFilter:
        value.statusFilter === "pending" ||
        value.statusFilter === "in_progress" ||
        value.statusFilter === "paused" ||
        value.statusFilter === "complete" ||
        value.statusFilter === "aborted"
          ? value.statusFilter
          : "all",
      viewport: viewportValue(value.viewport),
      dependencyViewport: viewportValue(value.dependencyViewport),
      dependencyUpstreamDepth: depthValue(
        value.dependencyUpstreamDepth,
      ),
      dependencyDownstreamDepth: depthValue(
        value.dependencyDownstreamDepth,
      ),
      replaySelectedSeq: replaySequenceValue(value.replaySelectedSeq),
      replayFollowLive:
        typeof value.replayFollowLive === "boolean"
          ? value.replayFollowLive
          : true,
      replayBranchFilter:
        typeof value.replayBranchFilter === "string"
          ? value.replayBranchFilter
          : "",
      replaySpeed: replaySpeedValue(value.replaySpeed),
      replayViewport: viewportValue(value.replayViewport),
      replayCollapsed: Array.isArray(value.replayCollapsed)
        ? value.replayCollapsed.filter(
            (item): item is string => typeof item === "string",
          )
        : [],
      annotations:
        value.annotations && typeof value.annotations === "object"
          ? Object.fromEntries(
              Object.entries(value.annotations).filter(
                ([key, annotation]) =>
                  key.length > 0 && typeof annotation === "string",
              ),
            )
          : {},
      settings: {
        wheelMode: settings.wheelMode === "zoom" ? "zoom" : "pan",
        panSensitivity: Math.min(
          1.5,
          Math.max(
            0.2,
            finiteNumber(
              settings.panSensitivity,
              DEFAULT_CANVAS_SETTINGS.panSensitivity,
            ),
          ),
        ),
        zoomSensitivity: Math.min(
          1.25,
          Math.max(
            0.1,
            finiteNumber(
              settings.zoomSensitivity,
              DEFAULT_CANVAS_SETTINGS.zoomSensitivity,
            ),
          ),
        ),
      },
    };
  } catch {
    return structuredClone(DEFAULT_SESSION);
  }
}

export function writeSession(value: ProjectMapSession): void {
  try {
    window.sessionStorage.setItem(sessionKey(), JSON.stringify(value));
  } catch {
    // The map remains usable when private-mode storage is unavailable.
  }
}
