import { beforeEach, describe, expect, it } from "vitest";
import {
  DEFAULT_SESSION,
  readSession,
  sessionKey,
  writeSession,
} from "./session";

describe("Project Map session", () => {
  beforeEach(() => {
    window.sessionStorage.clear();
  });

  it("defaults Dependency to direct depth without changing the namespace", () => {
    expect(sessionKey()).toBe("treework-project-map:v3:/");
    expect(readSession()).toEqual(DEFAULT_SESSION);
    expect(readSession().dependencyUpstreamDepth).toBe(1);
    expect(readSession().dependencyDownstreamDepth).toBe(1);
    expect(readSession().replayFollowLive).toBe(true);
    expect(readSession().replaySelectedSeq).toBeNull();
  });

  it("restores active view, focus, independent viewport, and expansion depth", () => {
    const value = {
      ...DEFAULT_SESSION,
      activeView: "dependency" as const,
      selected: "focus",
      inspectorOpen: false,
      dependencyViewport: { x: 12, y: 34, scale: 0.8 },
      dependencyUpstreamDepth: 3,
      dependencyDownstreamDepth: 4,
    };
    writeSession(value);
    expect(readSession()).toMatchObject(value);
  });

  it("migrates an existing Map session and clamps malformed depths", () => {
    window.sessionStorage.setItem(
      sessionKey(),
      JSON.stringify({
        selected: "legacy-selection",
        dependencyUpstreamDepth: 0,
        dependencyDownstreamDepth: 500,
      }),
    );
    expect(readSession()).toMatchObject({
      activeView: "map",
      selected: "legacy-selection",
      inspectorOpen: true,
      dependencyUpstreamDepth: 1,
      dependencyDownstreamDepth: 99,
      replayFollowLive: true,
      replaySelectedSeq: null,
    });
  });

  it("restores Replay position without changing the v3 namespace", () => {
    const value = {
      ...DEFAULT_SESSION,
      activeView: "replay" as const,
      replaySelectedSeq: 42,
      replayFollowLive: false,
      replayBranchFilter: "historical-branch",
      replaySpeed: 2 as const,
      replayViewport: { x: 18, y: 24, scale: 0.7 },
      replayCollapsed: ["settled-parent"],
    };
    writeSession(value);
    expect(readSession()).toMatchObject(value);
    expect(sessionKey()).toBe("treework-project-map:v3:/");
  });
});
