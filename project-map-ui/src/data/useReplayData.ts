import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { fetchReplay } from "./api";
import {
  changeReplayBranchFilter,
  filterReplayTransactions,
  reconcileReplayPosition,
  returnReplayToLive,
  selectReplaySequence,
  shouldCacheReplaySnapshot,
  type ReplayPosition,
} from "./replayState";
import type {
  ReplayResponse,
  ReplaySpeed,
} from "./types";

export type ReplayLoadPhase =
  | "idle"
  | "loading"
  | "refreshing"
  | "ready"
  | "unavailable";

interface UseReplayDataOptions {
  active: boolean;
  liveSignal: number;
  refreshEpoch: number;
  position: ReplayPosition;
  onPositionChange: (position: ReplayPosition) => void;
}

function samePosition(
  left: ReplayPosition,
  right: ReplayPosition,
): boolean {
  return (
    left.selectedSeq === right.selectedSeq &&
    left.branchFilter === right.branchFilter &&
    left.followLive === right.followLive &&
    left.speed === right.speed
  );
}

export function useReplayData({
  active,
  liveSignal,
  refreshEpoch,
  position,
  onPositionChange,
}: UseReplayDataOptions) {
  const [catalog, setCatalog] = useState<ReplayResponse | null>(null);
  const [catalogPhase, setCatalogPhase] =
    useState<ReplayLoadPhase>("idle");
  const [catalogError, setCatalogError] = useState("");
  const [snapshot, setSnapshot] = useState<ReplayResponse | null>(null);
  const [snapshotPhase, setSnapshotPhase] =
    useState<ReplayLoadPhase>("idle");
  const [snapshotError, setSnapshotError] = useState("");
  const [playing, setPlaying] = useState(false);
  const [catalogRetryEpoch, setCatalogRetryEpoch] = useState(0);
  const [snapshotRetryEpoch, setSnapshotRetryEpoch] = useState(0);
  const catalogRef = useRef<ReplayResponse | null>(null);
  const positionRef = useRef(position);
  const onPositionChangeRef = useRef(onPositionChange);
  const snapshotCache = useRef(new Map<number, ReplayResponse>());
  const catalogRequest = useRef(0);
  const snapshotRequest = useRef(0);

  positionRef.current = position;
  onPositionChangeRef.current = onPositionChange;

  const publishPosition = useCallback((next: ReplayPosition) => {
    if (!samePosition(positionRef.current, next)) {
      positionRef.current = next;
      onPositionChangeRef.current(next);
    }
  }, []);

  useEffect(() => {
    snapshotCache.current.clear();
  }, [refreshEpoch]);

  useEffect(() => {
    if (!active || liveSignal < 1) {
      return;
    }
    const request = ++catalogRequest.current;
    const controller = new AbortController();
    setCatalogPhase(catalogRef.current ? "refreshing" : "loading");
    setCatalogError("");
    void fetchReplay({}, controller.signal)
      .then((response) => {
        if (request !== catalogRequest.current) {
          return;
        }
        catalogRef.current = response;
        setCatalog(response);
        setCatalogPhase("ready");
        if (shouldCacheReplaySnapshot(response)) {
          snapshotCache.current.set(response.meta.at_event_seq, response);
        }
        publishPosition(
          reconcileReplayPosition(
            positionRef.current,
            response.transactions,
            response.meta.live_event_seq,
          ),
        );
      })
      .catch((error: unknown) => {
        if (
          request !== catalogRequest.current ||
          (error instanceof DOMException && error.name === "AbortError")
        ) {
          return;
        }
        setCatalogPhase(catalogRef.current ? "ready" : "unavailable");
        setCatalogError(
          error instanceof Error
            ? error.message
            : "Replay timeline could not be loaded.",
        );
      });
    return () => controller.abort();
  }, [
    active,
    catalogRetryEpoch,
    liveSignal,
    publishPosition,
    refreshEpoch,
  ]);

  useEffect(() => {
    const sequence = position.selectedSeq;
    if (!active || !catalog || sequence === null) {
      setSnapshot(null);
      setSnapshotPhase("idle");
      setSnapshotError("");
      return;
    }
    const cached = snapshotCache.current.get(sequence);
    if (cached) {
      setSnapshot(cached);
      setSnapshotPhase("ready");
      setSnapshotError("");
      return;
    }

    const request = ++snapshotRequest.current;
    const controller = new AbortController();
    setSnapshot(null);
    setSnapshotPhase("loading");
    setSnapshotError("");
    void fetchReplay(
      { at: sequence, after: sequence },
      controller.signal,
    )
      .then((response) => {
        if (request !== snapshotRequest.current) {
          return;
        }
        if (response.meta.at_event_seq !== sequence) {
          throw new Error(
            `Replay returned sequence ${response.meta.at_event_seq} for requested sequence ${sequence}.`,
          );
        }
        if (shouldCacheReplaySnapshot(response)) {
          snapshotCache.current.set(sequence, response);
        }
        setSnapshot(response);
        setSnapshotPhase("ready");
      })
      .catch((error: unknown) => {
        if (
          request !== snapshotRequest.current ||
          (error instanceof DOMException && error.name === "AbortError")
        ) {
          return;
        }
        setSnapshot(null);
        setSnapshotPhase("unavailable");
        setSnapshotError(
          error instanceof Error
            ? error.message
            : "The selected Replay snapshot could not be loaded.",
        );
      });
    return () => controller.abort();
  }, [
    active,
    catalog,
    position.selectedSeq,
    refreshEpoch,
    snapshotRetryEpoch,
  ]);

  useEffect(() => {
    if (!active) {
      setPlaying(false);
    }
  }, [active]);

  const visibleTransactions = useMemo(
    () =>
      filterReplayTransactions(
        catalog?.transactions ?? [],
        position.branchFilter,
      ),
    [catalog?.transactions, position.branchFilter],
  );
  const selectedIndex = visibleTransactions.findIndex(
    (transaction) => transaction.seq === position.selectedSeq,
  );
  const currentTransaction =
    selectedIndex >= 0 ? visibleTransactions[selectedIndex] : null;

  const selectIndex = useCallback(
    (index: number, pause = true) => {
      if (!catalog || index < 0 || index >= visibleTransactions.length) {
        return;
      }
      if (pause) {
        setPlaying(false);
      }
      publishPosition(
        selectReplaySequence(
          positionRef.current,
          visibleTransactions[index].seq,
          catalog.meta.live_event_seq,
        ),
      );
    },
    [catalog, publishPosition, visibleTransactions],
  );

  const step = useCallback(
    (offset: number) => {
      if (selectedIndex < 0) {
        return;
      }
      selectIndex(selectedIndex + offset);
    },
    [selectIndex, selectedIndex],
  );

  const setBranchFilter = useCallback(
    (branchFilter: string) => {
      if (!catalog) {
        return;
      }
      setPlaying(false);
      publishPosition(
        changeReplayBranchFilter(
          positionRef.current,
          catalog.transactions,
          branchFilter,
          catalog.meta.live_event_seq,
        ),
      );
    },
    [catalog, publishPosition],
  );

  const returnToLive = useCallback(() => {
    if (!catalog) {
      return;
    }
    setPlaying(false);
    publishPosition(
      returnReplayToLive(
        positionRef.current,
        catalog.transactions,
        catalog.meta.live_event_seq,
      ),
    );
  }, [catalog, publishPosition]);

  const setSpeed = useCallback(
    (speed: ReplaySpeed) => {
      publishPosition({ ...positionRef.current, speed });
    },
    [publishPosition],
  );

  const togglePlaying = useCallback(() => {
    if (playing) {
      setPlaying(false);
      return;
    }
    if (selectedIndex >= 0 && selectedIndex < visibleTransactions.length - 1) {
      if (snapshotPhase === "unavailable") {
        setSnapshotRetryEpoch((value) => value + 1);
      }
      setPlaying(true);
    }
  }, [
    playing,
    selectedIndex,
    snapshotPhase,
    visibleTransactions.length,
  ]);

  useEffect(() => {
    if (!playing || !active) {
      return;
    }
    if (selectedIndex < 0 || selectedIndex >= visibleTransactions.length - 1) {
      setPlaying(false);
      return;
    }
    if (snapshotPhase === "unavailable") {
      setPlaying(false);
      return;
    }
    if (
      snapshotPhase !== "ready" ||
      !snapshot ||
      snapshot.meta.at_event_seq !== position.selectedSeq
    ) {
      return;
    }
    const timer = window.setTimeout(
      () => selectIndex(selectedIndex + 1, false),
      Math.round(1200 / position.speed),
    );
    return () => window.clearTimeout(timer);
  }, [
    active,
    playing,
    position.selectedSeq,
    position.speed,
    selectIndex,
    selectedIndex,
    snapshot,
    snapshotPhase,
    visibleTransactions.length,
  ]);

  const atLive = Boolean(
    catalog &&
      position.followLive &&
      !position.branchFilter &&
      position.selectedSeq === catalog.meta.live_event_seq,
  );
  const liveAdvanced = Boolean(
    catalog &&
      !atLive &&
      position.selectedSeq !== null &&
      catalog.meta.live_event_seq > position.selectedSeq,
  );

  return {
    catalog,
    catalogPhase,
    catalogError,
    snapshot,
    snapshotPhase,
    snapshotError,
    visibleTransactions,
    selectedIndex,
    currentTransaction,
    playing,
    atLive,
    liveAdvanced,
    selectIndex,
    step,
    setBranchFilter,
    returnToLive,
    setSpeed,
    togglePlaying,
    retryCatalog: () => setCatalogRetryEpoch((value) => value + 1),
    retrySnapshot: () => setSnapshotRetryEpoch((value) => value + 1),
  };
}
