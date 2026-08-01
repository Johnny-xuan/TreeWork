import type { ReplayTransaction } from "../../data/types";

type BranchLabel = (id: string) => string;

function record(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

function number(value: unknown): number | null {
  return typeof value === "number" && Number.isSafeInteger(value)
    ? value
    : null;
}

function transition(value: unknown): { before: string; after: string } | null {
  const item = record(value);
  if (!item) {
    return null;
  }
  const before = text(item.before);
  const after = text(item.after);
  return before || after ? { before, after } : null;
}

function transitionLine(
  label: string,
  value: unknown,
  branchLabel?: BranchLabel,
): string | null {
  const item = transition(value);
  if (!item) {
    return null;
  }
  const format = branchLabel ?? ((candidate: string) => candidate || "none");
  return `${label}: ${format(item.before)} -> ${format(item.after)}.`;
}

function operationLines(
  changes: Record<string, unknown>,
  branchLabel: BranchLabel,
): string[] {
  if (!Array.isArray(changes.operations)) {
    return [];
  }
  return changes.operations.flatMap((candidate) => {
    const operation = record(candidate);
    if (!operation) {
      return [];
    }
    const kind = text(operation.kind);
    const branch = branchLabel(text(operation.branch));
    switch (kind) {
      case "create_branch": {
        const parent = branchLabel(text(operation.parent));
        const order = number(operation.sibling_order);
        return [
          `Created ${branch} under ${parent}${order === null ? "" : ` at sibling position ${order + 1}`}.`,
        ];
      }
      case "move_branch":
        return [
          `Moved ${branch} from ${branchLabel(text(operation.from))} to ${branchLabel(text(operation.to))}.`,
        ];
      case "update_metadata": {
        const fields = Array.isArray(operation.fields)
          ? operation.fields.filter(
              (field): field is string => typeof field === "string",
            )
          : [];
        return [
          `Updated ${branch} metadata${fields.length ? `: ${fields.join(", ")}` : ""}.`,
        ];
      }
      case "reorder_branch": {
        const from = number(operation.from);
        const to = number(operation.to);
        return [
          from === null || to === null
            ? `Reordered ${branch} among its siblings.`
            : `Reordered ${branch} from position ${from + 1} to ${to + 1}.`,
        ];
      }
      case "add_dependency":
        return [
          `${branch} now depends on ${branchLabel(text(operation.depends_on))}.`,
        ];
      case "remove_dependency":
        return [
          `${branch} no longer depends on ${branchLabel(text(operation.depends_on))}.`,
        ];
      default:
        return [];
    }
  });
}

export function replayEventLabel(eventType: string): string {
  const labels: Record<string, string> = {
    "project.initialized": "Project initialized",
    "alignment.started": "Alignment started",
    "alignment.accepted": "Alignment accepted",
    "tree.editing_started": "Tree editing started",
    "tree.editing_updated": "Tree editing updated",
    "tree.applied": "Tree applied",
    "branch.entered": "Branch entered",
    "branch.paused": "Branch paused",
    "branch.completed": "Branch completed",
    "branch.aborted": "Branch aborted",
    "verification.recorded": "Verification recorded",
  };
  return labels[eventType] ?? eventType;
}

export function formatReplayTime(value: string): string {
  const match = /^unix:(\d+)$/.exec(value);
  if (!match) {
    return value;
  }
  const milliseconds = Number(match[1]) * 1000;
  if (!Number.isFinite(milliseconds)) {
    return value;
  }
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(new Date(milliseconds));
}

export function formatReplayChanges(
  transaction: ReplayTransaction,
  branchLabel: BranchLabel = (id) => id || "none",
): string[] {
  const changes = record(transaction.changes);
  if (!changes) {
    return [
      transaction.replayable
        ? "No additional semantic change detail was published."
        : "Typed semantic changes are unavailable for this transaction.",
    ];
  }

  switch (transaction.type) {
    case "project.initialized": {
      const stage = record(changes.stage);
      const current = record(changes.current_branch);
      return [
        stage ? `Initialized project stage as ${text(stage.after)}.` : null,
        current
          ? `Initialized current branch as ${branchLabel(text(current.after))}.`
          : null,
      ].filter((line): line is string => Boolean(line));
    }
    case "alignment.started":
    case "alignment.accepted":
      return [
        transitionLine("Project stage", changes.stage),
      ].filter((line): line is string => Boolean(line));
    case "tree.editing_started":
    case "tree.editing_updated": {
      const editing = record(changes.editing);
      const stage = transitionLine("Project stage", changes.stage);
      const revision = editing ? number(editing.base_tree_revision) : null;
      const sequence = editing ? number(editing.base_event_seq) : null;
      return [
        stage,
        editing
          ? `Opened ${text(editing.mode) || "Tree"} editing from revision ${revision ?? "unknown"}, sequence ${sequence ?? "unknown"}.`
          : null,
      ].filter((line): line is string => Boolean(line));
    }
    case "tree.applied": {
      const result = record(changes.result);
      const revision = result ? number(result.tree_revision) : null;
      const operations = operationLines(changes, branchLabel);
      return [
        revision === null
          ? "Accepted the declarative Tree."
          : `Accepted Tree revision ${revision}.`,
        ...(operations.length
          ? operations
          : ["The accepted Tree contained no semantic topology changes."]),
      ];
    }
    case "branch.entered":
      return [
        transitionLine(
          "Current branch",
          changes.current_branch,
          branchLabel,
        ),
        transitionLine("Branch status", changes.status),
        transitionLine("Status reason", changes.reason),
      ].filter((line): line is string => Boolean(line));
    case "branch.paused":
    case "branch.aborted":
      return [
        transitionLine("Branch status", changes.status),
        transitionLine("Status reason", changes.reason),
      ].filter((line): line is string => Boolean(line));
    case "branch.completed": {
      const verification = record(changes.verification);
      return [
        transitionLine("Branch status", changes.status),
        transitionLine("Status reason", changes.reason),
        verification
          ? `Completion verification: ${text(verification.status) || "not recorded"}.`
          : null,
      ].filter((line): line is string => Boolean(line));
    }
    case "verification.recorded": {
      const evidence = record(changes.evidence);
      return [
        transitionLine("Verification", changes.verification),
        evidence && text(evidence.command)
          ? `Evidence command: ${text(evidence.command)}.`
          : null,
        evidence && text(evidence.result)
          ? `Evidence result: ${text(evidence.result)}.`
          : null,
        evidence && text(evidence.gap)
          ? `Coverage gap: ${text(evidence.gap)}.`
          : null,
      ].filter((line): line is string => Boolean(line));
    }
    default:
      return [
        transaction.replayable
          ? "No additional semantic change detail was published."
          : "Typed semantic changes are unavailable for this transaction.",
      ];
  }
}
