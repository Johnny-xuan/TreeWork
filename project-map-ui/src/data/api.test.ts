import { describe, expect, it } from "vitest";
import {
  parseInvalidation,
  parseProjection,
  parseReplayResponse,
} from "./api";

const projection = {
  schema_version: 1,
  tree_revision: 3,
  state_event_seq: 9,
  narrative_revision: "sha256:test",
  tree_editing: false,
  projected_at: "unix:1",
  health: { status: "ok", message: "" },
  project: {
    stage: "work_tree",
    current_branch: "root",
    topology_source: "accepted",
  },
  nodes: [
    {
      id: "root",
      parent: "",
      order: 0,
      title: "Root",
      purpose: "Coordinate the project.",
      spec: "spec.md",
      status: "in_progress",
      verification: "partial",
      status_reason: "",
      is_current: true,
      readiness: "active",
      depends_on: [],
      child_count: 0,
    },
  ],
  dependencies: [],
};

const replay = {
  schema_version: 1,
  meta: {
    live_event_seq: 3,
    at_event_seq: 2,
    checkpoint_event_seq: 2,
    earliest_replayable_seq: 1,
    tree_revision: 1,
    projected_at: "unix:4",
  },
  reconstruction: {
    status: "available",
    gaps: [],
  },
  state: {
    tree_editing: false,
    project: projection.project,
    nodes: projection.nodes,
    dependencies: [],
  },
  transactions: [
    {
      seq: 1,
      time: "unix:1",
      type: "project.initialized",
      subject: "root",
      message: "Initialized",
      tree_revision: 0,
      affected_subjects: ["root"],
      changes: {},
      replayable: true,
    },
    {
      seq: 2,
      time: "unix:2",
      type: "tree.applied",
      subject: "root",
      message: "Applied Tree",
      tree_revision: 1,
      affected_subjects: ["root"],
      changes: { operations: [] },
      replayable: true,
    },
  ],
};

describe("Project Map API parsing", () => {
  it("accepts the implemented current projection contract", () => {
    expect(parseProjection(projection).nodes[0].id).toBe("root");
  });

  it("rejects unsupported lifecycle values instead of inventing defaults", () => {
    expect(() =>
      parseProjection({
        ...projection,
        nodes: [{ ...projection.nodes[0], status: "blocked" }],
      }),
    ).toThrow("status is unsupported");
  });

  it("accepts only classified invalidation messages", () => {
    expect(
      parseInvalidation({
        schema_version: 1,
        kind: "project_map.invalidated",
        changes: ["topology", "state"],
        tree_revision: 4,
        state_event_seq: 10,
        narrative_revision: "sha256:next",
      }).changes,
    ).toEqual(["topology", "state"]);
    expect(() =>
      parseInvalidation({
        schema_version: 1,
        kind: "project_map.invalidated",
        changes: ["coordinates"],
        tree_revision: 4,
        state_event_seq: 10,
        narrative_revision: "sha256:next",
      }),
    ).toThrow("unsupported semantics");
  });

  it("accepts an available reconstructed Replay response", () => {
    const parsed = parseReplayResponse(replay);
    expect(parsed.meta.at_event_seq).toBe(2);
    expect(parsed.transactions.map((transaction) => transaction.seq)).toEqual([
      1, 2,
    ]);
    expect(parsed.state?.nodes[0].id).toBe("root");
  });

  it("rejects dishonest or unordered Replay responses", () => {
    expect(() =>
      parseReplayResponse({
        ...replay,
        state: null,
      }),
    ).toThrow("available replay reconstruction has no state");
    expect(() =>
      parseReplayResponse({
        ...replay,
        transactions: [...replay.transactions].reverse(),
      }),
    ).toThrow("not strictly ordered");
  });
});
