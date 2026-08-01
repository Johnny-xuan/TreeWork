import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  Dispatch,
  SetStateAction,
} from "react";
import type { ConnectionState } from "../../data/projectionState";
import {
  replayBranchOptions,
  shouldAnimateReplayTransition,
  type ReplayPosition,
} from "../../data/replayState";
import type {
  ProjectMapProjection,
  ReplayResponse,
} from "../../data/types";
import { useReplayData } from "../../data/useReplayData";
import type { ProjectMapSession } from "../../state/session";
import { MapView } from "../map/MapView";
import { ReplayTimeline } from "./ReplayTimeline";
import { ReplayTransactionDetail } from "./ReplayTransactionDetail";

interface ReplayViewProps {
  liveProjection: ProjectMapProjection;
  connection: ConnectionState;
  refreshEpoch: number;
  session: ProjectMapSession;
  setSession: Dispatch<SetStateAction<ProjectMapSession>>;
}

interface SceneTransition {
  from: ProjectMapProjection;
  key: number;
  durationMs: number;
}

function replayProjection(
  response: ReplayResponse | null,
  selectedSeq: number | null,
): ProjectMapProjection | null {
  if (
    !response ||
    response.meta.at_event_seq !== selectedSeq ||
    response.reconstruction.status !== "available" ||
    !response.state
  ) {
    return null;
  }
  return {
    schema_version: response.schema_version,
    tree_revision: response.meta.tree_revision,
    state_event_seq: response.meta.at_event_seq,
    narrative_revision: "",
    tree_editing: response.state.tree_editing,
    projected_at: response.meta.projected_at,
    health: { status: "ok", message: "" },
    project: response.state.project,
    nodes: response.state.nodes,
    dependencies: response.state.dependencies,
  };
}

function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () =>
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-reduced-motion: reduce)").matches,
  );

  useEffect(() => {
    if (typeof window.matchMedia !== "function") {
      return;
    }
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => setReduced(media.matches);
    media.addEventListener("change", update);
    return () => media.removeEventListener("change", update);
  }, []);
  return reduced;
}

function ReplayCoverage({
  response,
  sequence,
  error,
  loading,
  onRetry,
}: {
  response: ReplayResponse | null;
  sequence: number | null;
  error: string;
  loading: boolean;
  onRetry: () => void;
}) {
  if (loading) {
    return (
      <div className="replay-scene-state" role="status">
        <span className="replay-loading-mark" />
        <strong>Reconstructing sequence {sequence ?? ""}</strong>
      </div>
    );
  }
  const status = response?.reconstruction.status;
  const partial = status === "partial";
  return (
    <div
      className={`replay-scene-state ${partial ? "is-partial" : "is-unavailable"}`}
      role={partial ? "status" : "alert"}
    >
      <strong>
        {partial
          ? "Historical coverage is partial"
          : "Historical scene is unavailable"}
      </strong>
      <p>
        Sequence {sequence ?? "unknown"} is withheld because it cannot be
        reconstructed with complete accepted-state coverage.
      </p>
      {response?.reconstruction.gaps.length ? (
        <ul>
          {response.reconstruction.gaps.map((gap) => (
            <li key={`${gap.from_seq}:${gap.to_seq}:${gap.reason}`}>
              Seq {gap.from_seq}
              {gap.to_seq === gap.from_seq ? "" : `-${gap.to_seq}`}:{" "}
              {gap.reason}
            </li>
          ))}
        </ul>
      ) : (
        error && <p>{error}</p>
      )}
      {!partial && (
        <button type="button" onClick={onRetry}>
          Retry reconstruction
        </button>
      )}
    </div>
  );
}

export function ReplayView({
  liveProjection,
  connection,
  refreshEpoch,
  session,
  setSession,
}: ReplayViewProps) {
  const reducedMotion = useReducedMotion();
  const position = useMemo<ReplayPosition>(
    () => ({
      selectedSeq: session.replaySelectedSeq,
      branchFilter: session.replayBranchFilter,
      followLive: session.replayFollowLive,
      speed: session.replaySpeed,
    }),
    [
      session.replayBranchFilter,
      session.replayFollowLive,
      session.replaySelectedSeq,
      session.replaySpeed,
    ],
  );
  const updatePosition = useCallback(
    (next: ReplayPosition) =>
      setSession((previous) => ({
        ...previous,
        replaySelectedSeq: next.selectedSeq,
        replayBranchFilter: next.branchFilter,
        replayFollowLive: next.followLive,
        replaySpeed: next.speed,
      })),
    [setSession],
  );
  const replay = useReplayData({
    active: true,
    liveSignal: liveProjection.state_event_seq,
    refreshEpoch,
    position,
    onPositionChange: updatePosition,
  });
  const selectedResponse =
    replay.snapshot?.meta.at_event_seq === session.replaySelectedSeq
      ? replay.snapshot
      : null;
  const projection = useMemo(
    () =>
      replayProjection(selectedResponse, session.replaySelectedSeq),
    [selectedResponse, session.replaySelectedSeq],
  );
  const branchOptions = useMemo(
    () =>
      replayBranchOptions(
        replay.catalog?.transactions ?? [],
        liveProjection.nodes,
        projection?.nodes ?? [],
      ),
    [liveProjection.nodes, projection?.nodes, replay.catalog?.transactions],
  );
  const previousScene = useRef<{
    sequence: number;
    projection: ProjectMapProjection;
  } | null>(null);
  const replaySpeedRef = useRef(session.replaySpeed);
  replaySpeedRef.current = session.replaySpeed;
  const narrowViewportRecoveryHandled = useRef(false);
  const [transition, setTransition] =
    useState<SceneTransition | null>(null);
  const handleNarrowViewportRecovery = useCallback(() => {
    narrowViewportRecoveryHandled.current = true;
  }, []);
  const viewportFocusId = useMemo(() => {
    if (!projection) {
      return "";
    }
    const subject = replay.currentTransaction?.subject ?? "";
    return projection.nodes.some((node) => node.id === subject)
      ? subject
      : projection.project.current_branch;
  }, [projection, replay.currentTransaction?.subject]);

  useEffect(() => {
    const sequence = session.replaySelectedSeq;
    if (projection && sequence !== null) {
      if (previousScene.current?.sequence === sequence) {
        if (reducedMotion) {
          setTransition(null);
        }
        return;
      }
      const durationMs = Math.max(
        150,
        Math.round(360 / replaySpeedRef.current),
      );
      if (
        shouldAnimateReplayTransition(
          previousScene.current?.sequence ?? null,
          sequence,
          reducedMotion,
        ) &&
        previousScene.current
      ) {
        setTransition({
          from: previousScene.current.projection,
          key: sequence,
          durationMs,
        });
        const timer = window.setTimeout(
          () =>
            setTransition((current) =>
              current?.key === sequence ? null : current,
            ),
          durationMs + 40,
        );
        previousScene.current = { sequence, projection };
        return () => window.clearTimeout(timer);
      }
      setTransition(null);
      previousScene.current = { sequence, projection };
      return;
    }
    if (
      replay.snapshotPhase === "unavailable" ||
      (selectedResponse &&
        selectedResponse.reconstruction.status !== "available")
    ) {
      setTransition(null);
      previousScene.current = null;
    }
  }, [
    projection,
    reducedMotion,
    replay.snapshotPhase,
    selectedResponse,
    session.replaySelectedSeq,
  ]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented) {
        return;
      }
      const target = event.target;
      if (
        target instanceof Element &&
        target.closest(
          "input, textarea, select, button, [contenteditable='true'], .branch-node, [role='treeitem'], [role='option']",
        )
      ) {
        return;
      }
      if (
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey
      ) {
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        replay.step(-1);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        replay.step(1);
      } else if (event.key === " ") {
        event.preventDefault();
        replay.togglePlaying();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [replay.step, replay.togglePlaying]);

  if (!replay.catalog && replay.catalogPhase === "loading") {
    return (
      <section className="replay-view is-catalog-loading">
        <div className="replay-scene-state" role="status">
          <span className="replay-loading-mark" />
          <strong>Reading accepted transaction history</strong>
        </div>
      </section>
    );
  }

  if (!replay.catalog) {
    return (
      <section className="replay-view is-catalog-loading">
        <div className="replay-scene-state is-unavailable" role="alert">
          <strong>Replay timeline is unavailable</strong>
          <p>{replay.catalogError}</p>
          <button type="button" onClick={replay.retryCatalog}>
            Try again
          </button>
        </div>
      </section>
    );
  }

  return (
    <section
      className="replay-view"
      data-replay-seq={session.replaySelectedSeq ?? ""}
      data-replay-live={replay.atLive}
    >
      <div className="replay-stage" data-testid="replay-stage">
        {projection ? (
          <MapView
            projection={projection}
            selected={session.replayBranchFilter}
            query=""
            statusFilter="all"
            collapsed={session.replayCollapsed}
            viewport={session.replayViewport}
            settings={session.settings}
            locateNonce={0}
            fitNonce={0}
            viewportFocusId={viewportFocusId}
            recoverNarrowViewport={!narrowViewportRecoveryHandled.current}
            onNarrowViewportRecoveryHandled={handleNarrowViewportRecovery}
            transitionFrom={transition?.from}
            transitionKey={transition?.key}
            transitionDurationMs={transition?.durationMs}
            onSelect={replay.setBranchFilter}
            onClearSelection={() => replay.setBranchFilter("")}
            onCollapsedChange={(replayCollapsed) =>
              setSession((previous) => ({
                ...previous,
                replayCollapsed,
              }))
            }
            onViewportChange={(replayViewport) =>
              setSession((previous) => ({
                ...previous,
                replayViewport,
              }))
            }
          />
        ) : (
          <ReplayCoverage
            response={selectedResponse}
            sequence={session.replaySelectedSeq}
            error={replay.snapshotError}
            loading={
              !selectedResponse &&
              replay.snapshotPhase !== "unavailable"
            }
            onRetry={replay.retrySnapshot}
          />
        )}

        {(replay.catalogError ||
          connection === "reconnecting" ||
          connection === "offline") && (
          <div className="replay-catalog-warning" role="status">
            {replay.catalogError ||
              (connection === "offline"
                ? "Live updates are offline."
                : "Live updates are reconnecting.")}
          </div>
        )}
      </div>

      <ReplayTimeline
        transactions={replay.visibleTransactions}
        selectedIndex={replay.selectedIndex}
        selectedSequence={session.replaySelectedSeq}
        selectedTransaction={replay.currentTransaction}
        meta={replay.catalog.meta}
        branchOptions={branchOptions}
        branchFilter={session.replayBranchFilter}
        speed={session.replaySpeed}
        playing={replay.playing}
        atLive={replay.atLive}
        liveAdvanced={replay.liveAdvanced}
        refreshing={replay.catalogPhase === "refreshing"}
        onSelectIndex={replay.selectIndex}
        onStep={replay.step}
        onTogglePlaying={replay.togglePlaying}
        onBranchFilterChange={replay.setBranchFilter}
        onSpeedChange={replay.setSpeed}
        onReturnToLive={replay.returnToLive}
      />

      <ReplayTransactionDetail
        transaction={replay.currentTransaction}
        branchOptions={branchOptions}
        knownNodes={projection?.nodes ?? liveProjection.nodes}
      />
    </section>
  );
}
