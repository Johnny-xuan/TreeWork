import { describe, expect, it } from "vitest";
import type { ReplayTransaction } from "../../data/types";
import { formatReplayChanges, replayEventLabel } from "./changes";

function transaction(
  type: string,
  changes: unknown,
): ReplayTransaction {
  return {
    seq: 7,
    time: "unix:7",
    type,
    subject: "alpha",
    message: "Accepted change",
    tree_revision: 2,
    affected_subjects: ["alpha"],
    changes,
    replayable: true,
    replayability_reason: null,
  };
}

describe("Replay semantic change text", () => {
  it("formats grouped Tree Apply operations without exposing raw JSON", () => {
    const lines = formatReplayChanges(
      transaction("tree.applied", {
        result: { tree_revision: 2 },
        operations: [
          {
            kind: "move_branch",
            branch: "alpha",
            from: "root",
            to: "delivery",
          },
          {
            kind: "add_dependency",
            branch: "alpha",
            depends_on: "foundation",
          },
        ],
      }),
    );
    expect(lines).toEqual([
      "Accepted Tree revision 2.",
      "Moved alpha from root to delivery.",
      "alpha now depends on foundation.",
    ]);
    expect(lines.join(" ")).not.toContain("{");
  });

  it("formats lifecycle and verification transitions", () => {
    expect(
      formatReplayChanges(
        transaction("branch.paused", {
          status: { before: "in_progress", after: "paused" },
          reason: { before: "", after: "Waiting for review" },
        }),
      ),
    ).toEqual([
      "Branch status: in_progress -> paused.",
      "Status reason: none -> Waiting for review.",
    ]);
    expect(
      formatReplayChanges(
        transaction("verification.recorded", {
          verification: { before: "unverified", after: "verified" },
          evidence: {
            command: "npm test",
            result: "passed",
            gap: "none",
          },
        }),
      ),
    ).toContain("Evidence result: passed.");
  });

  it("reports unsupported legacy changes honestly", () => {
    const legacy = {
      ...transaction("branch.entered", null),
      replayable: false,
      replayability_reason: "legacy event",
    };
    expect(formatReplayChanges(legacy)).toEqual([
      "Typed semantic changes are unavailable for this transaction.",
    ]);
    expect(replayEventLabel("branch.entered")).toBe("Branch entered");
  });
});
