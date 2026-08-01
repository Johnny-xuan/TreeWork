import { describe, expect, it } from "vitest";
import {
  changeReplayBranchFilter,
  filterReplayTransactions,
  reconcileReplayPosition,
  replayBranchOptions,
  returnReplayToLive,
  sequenceForFilteredTimeline,
  shouldAnimateReplayTransition,
  shouldCacheReplaySnapshot,
  type ReplayPosition,
} from "./replayState";
import type {
  ProjectMapNode,
  ReplayResponse,
  ReplayTransaction,
} from "./types";

function transaction(
  seq: number,
  subject: string,
  affected: string[] = [subject],
): ReplayTransaction {
  return {
    seq,
    time: `unix:${seq}`,
    type: seq === 2 ? "tree.applied" : "branch.entered",
    subject,
    message: `Transaction ${seq}`,
    tree_revision: 1,
    affected_subjects: affected,
    changes:
      seq === 2
        ? {
            operations: [
              { kind: "create_branch", branch: "alpha", parent: "root" },
              { kind: "add_dependency", branch: "alpha", depends_on: "root" },
            ],
          }
        : {},
    replayable: true,
    replayability_reason: null,
  };
}

const transactions = [
  transaction(1, "root"),
  transaction(2, "root", ["alpha", "historical"]),
  transaction(3, "beta"),
  transaction(4, "alpha"),
];

const position: ReplayPosition = {
  selectedSeq: 3,
  branchFilter: "",
  followLive: false,
  speed: 1,
};

describe("Replay timeline state", () => {
  it("keeps one grouped transaction as one timeline step", () => {
    const filtered = filterReplayTransactions(transactions, "alpha");
    expect(filtered.map((item) => item.seq)).toEqual([2, 4]);
    expect(filtered).toHaveLength(2);
    expect(
      (filtered[0].changes as { operations: unknown[] }).operations,
    ).toHaveLength(2);
  });

  it("chooses the nearest matching step at or before the current sequence", () => {
    expect(
      changeReplayBranchFilter(position, transactions, "alpha", 4),
    ).toMatchObject({
      branchFilter: "alpha",
      selectedSeq: 2,
      followLive: false,
    });
  });

  it("chooses the earliest matching step when no earlier match exists", () => {
    expect(
      sequenceForFilteredTimeline(
        filterReplayTransactions(transactions, "alpha"),
        1,
      ),
    ).toBe(2);
  });

  it("does not jump to global Live when a filter is cleared", () => {
    const filtered = {
      ...position,
      selectedSeq: 2,
      branchFilter: "alpha",
    };
    expect(
      changeReplayBranchFilter(filtered, transactions, "", 4),
    ).toMatchObject({
      selectedSeq: 2,
      branchFilter: "",
      followLive: false,
    });
  });

  it("preserves the scene cursor for a branch with no matching transaction", () => {
    expect(
      changeReplayBranchFilter(
        position,
        transactions,
        "checkpoint-only",
        4,
      ),
    ).toMatchObject({
      selectedSeq: 3,
      branchFilter: "checkpoint-only",
      followLive: false,
    });
    expect(
      reconcileReplayPosition(
        {
          ...position,
          branchFilter: "checkpoint-only",
        },
        transactions,
        4,
      ),
    ).toMatchObject({
      selectedSeq: 3,
      branchFilter: "checkpoint-only",
      followLive: false,
    });
  });

  it("clears filtering only for explicit Return to Live", () => {
    expect(
      returnReplayToLive(
        { ...position, branchFilter: "alpha", selectedSeq: 2 },
        transactions,
        4,
      ),
    ).toMatchObject({
      selectedSeq: 4,
      branchFilter: "",
      followLive: true,
    });
  });

  it("follows appended catalog entries only from global Live", () => {
    expect(
      reconcileReplayPosition(
        { ...position, selectedSeq: 3, followLive: true },
        transactions,
        4,
      ).selectedSeq,
    ).toBe(4);
    expect(
      reconcileReplayPosition(position, transactions, 4),
    ).toMatchObject({
      selectedSeq: 3,
      followLive: false,
    });
  });

  it("uses only reconstructed or live nodes to title historical branch IDs", () => {
    const node = (id: string, title: string): ProjectMapNode => ({
      id,
      parent: id === "root" ? "" : "root",
      order: 0,
      title,
      purpose: "",
      spec: null,
      status: "pending",
      verification: "unverified",
      status_reason: "",
      is_current: false,
      readiness: "ready",
      depends_on: [],
      child_count: 0,
    });
    const options = replayBranchOptions(
      transactions,
      [
        node("alpha", "Live Alpha"),
        node("live-only", "Live Only"),
      ],
      [
        node("alpha", "Historical Alpha"),
        node("checkpoint-only", "Checkpoint Only"),
      ],
    );
    expect(options.find((item) => item.id === "alpha")?.label).toBe(
      "Historical Alpha · alpha",
    );
    expect(options.find((item) => item.id === "historical")?.label).toBe(
      "historical",
    );
    expect(options.find((item) => item.id === "live-only")?.label).toBe(
      "Live Only · live-only",
    );
    expect(
      options.find((item) => item.id === "checkpoint-only")?.label,
    ).toBe("Checkpoint Only · checkpoint-only");
  });

  it("does not cache unavailable snapshots or animate sequence gaps", () => {
    const response = {
      reconstruction: { status: "unavailable" },
    } as ReplayResponse;
    expect(shouldCacheReplaySnapshot(response)).toBe(false);
    expect(
      shouldCacheReplaySnapshot({
        reconstruction: { status: "partial" },
      } as ReplayResponse),
    ).toBe(true);
    expect(shouldAnimateReplayTransition(2, 3, false)).toBe(true);
    expect(shouldAnimateReplayTransition(2, 4, false)).toBe(false);
    expect(shouldAnimateReplayTransition(2, 3, true)).toBe(false);
  });
});
