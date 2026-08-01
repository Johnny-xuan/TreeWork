import type { ProjectMapNode } from "../data/types";

export interface StatusPresentation {
  label: string;
  symbol: string;
  className: string;
}

export function statusPresentation(
  node: ProjectMapNode,
): StatusPresentation {
  if (node.is_current) {
    return { label: "Current", symbol: "◎", className: "is-current" };
  }
  switch (node.status) {
    case "in_progress":
      return {
        label: "In progress",
        symbol: "●",
        className: "is-in-progress",
      };
    case "complete":
      return { label: "Complete", symbol: "✓", className: "is-complete" };
    case "paused":
      return { label: "Paused", symbol: "Ⅱ", className: "is-paused" };
    case "aborted":
      return { label: "Aborted", symbol: "■", className: "is-aborted" };
    case "pending":
      return node.readiness === "ready"
        ? { label: "Ready", symbol: "→", className: "is-ready" }
        : { label: "Waiting", symbol: "○", className: "is-waiting" };
  }
}

export function verificationPresentation(
  verification: ProjectMapNode["verification"],
): { label: string; symbol: string; className: string } {
  switch (verification) {
    case "verified":
      return { label: "Verified", symbol: "◆", className: "is-verified" };
    case "partial":
      return {
        label: "Partially verified",
        symbol: "◐",
        className: "is-partial",
      };
    case "failed":
      return {
        label: "Verification failed",
        symbol: "◈",
        className: "is-failed",
      };
    case "unverified":
      return {
        label: "Unverified",
        symbol: "◇",
        className: "is-unverified",
      };
  }
}
