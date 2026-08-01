import { useCallback, useEffect, useReducer, useRef } from "react";
import { fetchProjection, parseInvalidation } from "./api";
import {
  initialProjectionState,
  projectionReducer,
} from "./projectionState";
import type { InvalidationCategory } from "./types";

const INVALIDATION_DEBOUNCE_MS = 48;
export const CONNECTION_OFFLINE_AFTER_MS = 5000;

export function useProjectMapData() {
  const [state, dispatch] = useReducer(
    projectionReducer,
    initialProjectionState,
  );
  const requestSequence = useRef(0);
  const activeController = useRef<AbortController | null>(null);

  const refresh = useCallback(async () => {
    const request = ++requestSequence.current;
    activeController.current?.abort();
    const controller = new AbortController();
    activeController.current = controller;
    dispatch({ type: "projectionRequested" });
    try {
      const projection = await fetchProjection(controller.signal);
      if (request === requestSequence.current) {
        dispatch({ type: "projectionReceived", projection });
      }
    } catch (error) {
      if (request === requestSequence.current) {
        dispatch({
          type: "projectionFailed",
          message:
            error instanceof Error
              ? error.message
              : "Project Map could not be loaded.",
        });
      }
    }
  }, []);

  useEffect(() => {
    void refresh();
    return () => activeController.current?.abort();
  }, [refresh, state.refreshEpoch]);

  useEffect(() => {
    if (!("EventSource" in window)) {
      dispatch({ type: "connectionChanged", connection: "offline" });
      return;
    }

    let hasOpened = false;
    let offlineLatched = false;
    let timer = 0;
    let offlineTimer = 0;
    const pending = new Set<InvalidationCategory>();
    const source = new EventSource("/api/project-map/events");

    const clearOfflineTimer = () => {
      if (offlineTimer) {
        window.clearTimeout(offlineTimer);
        offlineTimer = 0;
      }
    };
    const markReconnecting = () => {
      if (offlineLatched) {
        return;
      }
      dispatch({ type: "connectionChanged", connection: "reconnecting" });
      if (!offlineTimer) {
        offlineTimer = window.setTimeout(() => {
          offlineTimer = 0;
          offlineLatched = true;
          dispatch({ type: "connectionChanged", connection: "offline" });
        }, CONNECTION_OFFLINE_AFTER_MS);
      }
    };
    const flush = () => {
      timer = 0;
      const changes = new Set(pending);
      pending.clear();
      if (changes.has("narrative")) {
        dispatch({ type: "narrativeInvalidated" });
      }
      if (
        changes.has("topology") ||
        changes.has("state") ||
        changes.has("events") ||
        changes.has("health")
      ) {
        void refresh();
      }
    };

    source.onopen = () => {
      clearOfflineTimer();
      offlineLatched = false;
      dispatch({ type: "connectionChanged", connection: "live" });
      void refresh();
      dispatch({ type: "narrativeInvalidated" });
      hasOpened = true;
    };
    source.onerror = markReconnecting;
    source.addEventListener("invalidate", (event) => {
      try {
        const invalidation = parseInvalidation(
          JSON.parse((event as MessageEvent<string>).data),
        );
        invalidation.changes.forEach((change) => pending.add(change));
        if (!timer) {
          timer = window.setTimeout(flush, INVALIDATION_DEBOUNCE_MS);
        }
      } catch {
        void refresh();
      }
    });
    const onOffline = () => {
      clearOfflineTimer();
      offlineLatched = true;
      dispatch({ type: "connectionChanged", connection: "offline" });
    };
    const onOnline = () => {
      markReconnecting();
      void refresh();
      dispatch({ type: "narrativeInvalidated" });
    };
    const onVisibilityChange = () => {
      if (document.visibilityState === "visible" && hasOpened) {
        void refresh();
        dispatch({ type: "narrativeInvalidated" });
      }
    };
    window.addEventListener("offline", onOffline);
    window.addEventListener("online", onOnline);
    document.addEventListener("visibilitychange", onVisibilityChange);

    return () => {
      if (timer) {
        window.clearTimeout(timer);
      }
      clearOfflineTimer();
      window.removeEventListener("offline", onOffline);
      window.removeEventListener("online", onOnline);
      document.removeEventListener("visibilitychange", onVisibilityChange);
      source.close();
    };
  }, [refresh]);

  const manualRefresh = useCallback(() => {
    dispatch({ type: "manualRefresh" });
  }, []);

  return { ...state, refresh: manualRefresh };
}
