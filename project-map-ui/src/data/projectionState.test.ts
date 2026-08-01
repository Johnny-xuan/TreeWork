import { describe, expect, it } from "vitest";
import {
  initialProjectionState,
  projectionReducer,
} from "./projectionState";
import type { ProjectMapProjection } from "./types";

const projection: ProjectMapProjection = {
  schema_version: 1,
  tree_revision: 1,
  state_event_seq: 2,
  narrative_revision: "sha256:test",
  tree_editing: false,
  projected_at: "unix:1",
  health: { status: "ok", message: "" },
  project: {
    stage: "work_tree",
    current_branch: "root",
    topology_source: "accepted",
  },
  nodes: [],
  dependencies: [],
};

describe("projectionReducer", () => {
  it("keeps the last good projection when a refresh fails", () => {
    const ready = projectionReducer(initialProjectionState, {
      type: "projectionReceived",
      projection,
    });
    const failed = projectionReducer(ready, {
      type: "projectionFailed",
      message: "partial publication",
    });
    expect(failed.phase).toBe("ready");
    expect(failed.projection).toBe(projection);
    expect(failed.error).toBe("partial publication");
  });

  it("reports unavailable when no accepted projection was ever read", () => {
    const failed = projectionReducer(initialProjectionState, {
      type: "projectionFailed",
      message: "missing state",
    });
    expect(failed.phase).toBe("unavailable");
    expect(failed.projection).toBeNull();
  });

  it("tracks narrative invalidation independently from topology", () => {
    const next = projectionReducer(initialProjectionState, {
      type: "narrativeInvalidated",
    });
    expect(next.narrativeEpoch).toBe(1);
    expect(next.refreshEpoch).toBe(0);
  });
});
