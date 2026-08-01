import { useEffect, useState } from "react";
import { fetchBranchDetail } from "./api";
import type { BranchDetail } from "./types";

export interface BranchDetailState {
  phase: "idle" | "loading" | "ready" | "stale" | "unavailable";
  detail: BranchDetail | null;
  error: string;
}

const initialState: BranchDetailState = {
  phase: "idle",
  detail: null,
  error: "",
};

export function useBranchDetail(
  branchId: string,
  narrativeEpoch: number,
  stateEventSeq: number,
  treeRevision: number,
): BranchDetailState {
  const [state, setState] = useState<BranchDetailState>(initialState);

  useEffect(() => {
    if (!branchId) {
      setState(initialState);
      return;
    }
    const controller = new AbortController();
    setState((previous) => ({
      phase:
        previous.detail?.branch.id === branchId ? "stale" : "loading",
      detail:
        previous.detail?.branch.id === branchId ? previous.detail : null,
      error: "",
    }));
    void fetchBranchDetail(branchId, controller.signal)
      .then((detail) => {
        setState({ phase: "ready", detail, error: "" });
      })
      .catch((error) => {
        if (controller.signal.aborted) {
          return;
        }
        setState((previous) => ({
          phase: previous.detail ? "stale" : "unavailable",
          detail: previous.detail,
          error:
            error instanceof Error
              ? error.message
              : "Branch detail is unavailable.",
        }));
      });
    return () => controller.abort();
  }, [branchId, narrativeEpoch, stateEventSeq, treeRevision]);

  return state;
}
