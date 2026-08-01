import type { ProjectMapProjection } from "./types";

export type ConnectionState =
  | "connecting"
  | "live"
  | "reconnecting"
  | "offline";

export interface ProjectionState {
  phase: "loading" | "ready" | "unavailable";
  projection: ProjectMapProjection | null;
  connection: ConnectionState;
  error: string;
  narrativeEpoch: number;
  refreshEpoch: number;
}

export type ProjectionAction =
  | { type: "projectionRequested" }
  | { type: "projectionReceived"; projection: ProjectMapProjection }
  | { type: "projectionFailed"; message: string }
  | { type: "connectionChanged"; connection: ConnectionState }
  | { type: "narrativeInvalidated" }
  | { type: "manualRefresh" };

export const initialProjectionState: ProjectionState = {
  phase: "loading",
  projection: null,
  connection: "connecting",
  error: "",
  narrativeEpoch: 0,
  refreshEpoch: 0,
};

export function projectionReducer(
  state: ProjectionState,
  action: ProjectionAction,
): ProjectionState {
  switch (action.type) {
    case "projectionRequested":
      return state.projection
        ? { ...state, error: "" }
        : { ...state, phase: "loading", error: "" };
    case "projectionReceived":
      return {
        ...state,
        phase: "ready",
        projection: action.projection,
        error: "",
      };
    case "projectionFailed":
      return {
        ...state,
        phase: state.projection ? "ready" : "unavailable",
        error: action.message,
      };
    case "connectionChanged":
      return { ...state, connection: action.connection };
    case "narrativeInvalidated":
      return { ...state, narrativeEpoch: state.narrativeEpoch + 1 };
    case "manualRefresh":
      return { ...state, refreshEpoch: state.refreshEpoch + 1 };
  }
}
