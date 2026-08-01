import {
  useCallback,
  useEffect,
  useMemo,
  useState,
} from "react";
import { useBranchDetail } from "../data/useBranchDetail";
import { useProjectMapData } from "../data/useProjectMapData";
import type { LifecycleStatus } from "../data/types";
import { useProjectMapSession } from "../state/useProjectMapSession";
import type { ProjectMapView } from "../state/session";
import { Inspector } from "../inspector/Inspector";
import { DependencyView } from "../views/dependency/DependencyView";
import { CanvasSettingsPanel } from "../views/map/CanvasSettingsPanel";
import { MapView } from "../views/map/MapView";
import { ReplayView } from "../views/replay/ReplayView";
import { ProjectionNotice } from "./ProjectionNotice";
import { TopBar } from "./TopBar";

export function App() {
  const data = useProjectMapData();
  const [session, setSession] = useProjectMapSession();
  const [query, setQuery] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [locateNonce, setLocateNonce] = useState(0);
  const [fitNonce, setFitNonce] = useState(0);
  const [branchHistory, setBranchHistory] = useState<{
    back: string[];
    forward: string[];
  }>({ back: [], forward: [] });
  const projection = data.projection;
  const selectedExists = Boolean(
    projection?.nodes.some((node) => node.id === session.selected),
  );
  const dependencyFocusId =
    (selectedExists ? session.selected : "") ||
    projection?.project.current_branch ||
    projection?.nodes[0]?.id ||
    "";
  const branchFocusId = dependencyFocusId;
  const selectedNode = useMemo(
    () =>
      projection?.nodes.find((node) => node.id === session.selected) ?? null,
    [projection?.nodes, session.selected],
  );
  const detail = useBranchDetail(
    session.activeView !== "replay" && session.inspectorOpen
      ? selectedNode?.id ?? ""
      : "",
    data.narrativeEpoch,
    projection?.state_event_seq ?? 0,
    projection?.tree_revision ?? 0,
  );

  const revealMapBranch = useCallback(
    (id: string, collapsedBranches: readonly string[]): string[] => {
      if (!projection) {
        return [...collapsedBranches];
      }
      const byId = new Map(projection.nodes.map((node) => [node.id, node]));
      const collapsed = new Set(collapsedBranches);
      let cursor = byId.get(id);
      while (cursor?.parent) {
        collapsed.delete(cursor.parent);
        cursor = byId.get(cursor.parent);
      }
      return [...collapsed];
    },
    [projection],
  );

  const applyBranchFocus = useCallback(
    (id: string, locate = true) => {
      if (!projection?.nodes.some((node) => node.id === id)) {
        return;
      }
      setSession((previous) => ({
        ...previous,
        selected: id,
        inspectorOpen: true,
        collapsed:
          previous.activeView === "map"
            ? revealMapBranch(id, previous.collapsed)
            : previous.collapsed,
      }));
      if (locate) {
        setLocateNonce((value) => value + 1);
      }
    },
    [projection, revealMapBranch, setSession],
  );

  const navigateToBranch = useCallback(
    (id: string, locate = true) => {
      if (
        !projection?.nodes.some((node) => node.id === id) ||
        id === branchFocusId
      ) {
        applyBranchFocus(id, locate);
        return;
      }
      if (branchFocusId) {
        setBranchHistory((previous) => ({
          back: [...previous.back, branchFocusId].slice(-50),
          forward: [],
        }));
      }
      applyBranchFocus(id, locate);
    },
    [applyBranchFocus, branchFocusId, projection],
  );

  const goBack = () => {
    const target = branchHistory.back.at(-1);
    if (!target) {
      return;
    }
    setBranchHistory((previous) => ({
      back: previous.back.slice(0, -1),
      forward:
        branchFocusId && branchFocusId !== target
          ? [branchFocusId, ...previous.forward].slice(0, 50)
          : previous.forward,
    }));
    applyBranchFocus(target);
  };

  const goForward = () => {
    const target = branchHistory.forward[0];
    if (!target) {
      return;
    }
    setBranchHistory((previous) => ({
      back:
        branchFocusId && branchFocusId !== target
          ? [...previous.back, branchFocusId].slice(-50)
          : previous.back,
      forward: previous.forward.slice(1),
    }));
    applyBranchFocus(target);
  };

  useEffect(() => {
    if (!projection) {
      return;
    }
    const selectedValid = projection.nodes.some(
      (node) => node.id === session.selected,
    );
    if (session.activeView === "dependency" && !selectedValid) {
      setSession((previous) => ({
        ...previous,
        selected:
          projection.nodes.find(
            (node) => node.id === projection.project.current_branch,
          )?.id ??
          projection.nodes[0]?.id ??
          "",
        inspectorOpen: false,
      }));
    } else if (session.selected && !selectedValid) {
      setSession((previous) => ({
        ...previous,
        selected: "",
        inspectorOpen: false,
      }));
    }
  }, [
    projection,
    session.activeView,
    session.selected,
    setSession,
  ]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey
      ) {
        return;
      }
      const target = event.target;
      const inControl =
        target instanceof Element &&
        Boolean(
          target.closest(
          "input, textarea, select, button, [contenteditable='true'], [role='slider'], [role='option']",
          ),
        );
      if (
        event.key === "/" &&
        !inControl &&
        session.activeView !== "replay"
      ) {
        event.preventDefault();
        document.querySelector<HTMLInputElement>("#branchSearch")?.focus();
      } else if (event.key === "Escape") {
        if (settingsOpen) {
          setSettingsOpen(false);
        } else if (session.inspectorOpen) {
          setSession((previous) => ({
            ...previous,
            selected:
              previous.activeView === "dependency"
                ? previous.selected
                : "",
            inspectorOpen: false,
          }));
        }
      } else if (
        !inControl &&
        session.activeView !== "replay" &&
        (event.key === "l" || event.key === "L")
      ) {
        if (projection) {
          navigateToBranch(projection.project.current_branch);
        }
      } else if (
        !inControl &&
        session.activeView !== "replay" &&
        (event.key === "f" || event.key === "F")
      ) {
        setFitNonce((value) => value + 1);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    projection,
    navigateToBranch,
    session.activeView,
    session.inspectorOpen,
    setSession,
    settingsOpen,
  ]);

  const selectSearchResult = () => {
    if (!projection || !query.trim()) {
      return;
    }
    const normalized = query.trim().toLocaleLowerCase();
    const match = projection.nodes.find((node) =>
      [node.id, node.title, node.purpose]
        .join(" ")
        .toLocaleLowerCase()
        .includes(normalized),
    );
    if (!match) {
      return;
    }
    navigateToBranch(match.id);
  };

  const changeView = (view: ProjectMapView) => {
    if (view === session.activeView || !projection) {
      return;
    }
    const focusId =
      (selectedExists ? session.selected : "") ||
      projection.project.current_branch ||
      projection.nodes[0]?.id ||
      "";
    setSettingsOpen(false);
    if (view === "replay") {
      setSession((previous) => ({
        ...previous,
        activeView: "replay",
        inspectorOpen: false,
      }));
      return;
    }
    if (view === "map") {
      setSession((previous) => ({
        ...previous,
        activeView: "map",
        selected: focusId,
        inspectorOpen: Boolean(focusId),
        collapsed: revealMapBranch(focusId, previous.collapsed),
      }));
      setLocateNonce((value) => value + 1);
      return;
    }
    setSession((previous) => ({
      ...previous,
      activeView: "dependency",
      selected: focusId,
    }));
    setLocateNonce((value) => value + 1);
  };

  return (
    <div className="project-map-app">
      <TopBar
        projection={projection}
        activeView={session.activeView}
        breadcrumbBranchId={
          session.activeView === "replay"
            ? ""
            : session.activeView === "dependency"
            ? dependencyFocusId
            : projection?.project.current_branch ?? ""
        }
        query={query}
        statusFilter={session.statusFilter}
        canGoBack={
          session.activeView !== "replay" && branchHistory.back.length > 0
        }
        canGoForward={
          session.activeView !== "replay" && branchHistory.forward.length > 0
        }
        onViewChange={changeView}
        onGoBack={goBack}
        onGoForward={goForward}
        onQueryChange={setQuery}
        onSearchCommit={selectSearchResult}
        onStatusFilterChange={(status: LifecycleStatus | "all") =>
          setSession((previous) => ({ ...previous, statusFilter: status }))
        }
        onRefresh={data.refresh}
        onLocateCurrent={() => {
          if (projection) {
            navigateToBranch(projection.project.current_branch);
          }
        }}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      {session.activeView !== "replay" && (
        <ProjectionNotice
          projection={projection}
          connection={data.connection}
          error={data.error}
        />
      )}

      <main className="project-map-workspace">
        {data.phase === "loading" && !projection && (
          <div className="map-loading" role="status">
            <span />
            <strong>Reading accepted project state…</strong>
          </div>
        )}
        {data.phase === "unavailable" && !projection && (
          <div className="map-unavailable" role="alert">
            <strong>Project Map is unavailable</strong>
            <p>{data.error || "No coherent accepted state could be read."}</p>
            <button type="button" onClick={data.refresh}>
              Try again
            </button>
          </div>
        )}
        {projection && projection.nodes.length === 0 && (
          <div className="map-unavailable" role="status">
            <strong>No accepted branches</strong>
            <p>The project has not published a visible root branch.</p>
          </div>
        )}
        {projection &&
          projection.nodes.length > 0 &&
          session.activeView === "map" && (
          <MapView
            projection={projection}
            selected={session.selected}
            query={query}
            statusFilter={session.statusFilter}
            collapsed={session.collapsed}
            viewport={session.viewport}
            settings={session.settings}
            locateNonce={locateNonce}
            fitNonce={fitNonce}
            onSelect={(id) => navigateToBranch(id, false)}
            onClearSelection={() =>
              setSession((previous) => ({
                ...previous,
                selected: "",
                inspectorOpen: false,
              }))
            }
            onCollapsedChange={(collapsed) =>
              setSession((previous) => ({ ...previous, collapsed }))
            }
            onViewportChange={(viewport) =>
              setSession((previous) => ({ ...previous, viewport }))
            }
          />
        )}

        {projection &&
          projection.nodes.length > 0 &&
          session.activeView === "dependency" &&
          dependencyFocusId && (
            <DependencyView
              projection={projection}
              focusId={dependencyFocusId}
              query={query}
              statusFilter={session.statusFilter}
              upstreamDepth={session.dependencyUpstreamDepth}
              downstreamDepth={session.dependencyDownstreamDepth}
              viewport={session.dependencyViewport}
              settings={session.settings}
              locateNonce={locateNonce}
              fitNonce={fitNonce}
              onSelect={(id) => navigateToBranch(id, false)}
              onViewportChange={(dependencyViewport) =>
                setSession((previous) => ({
                  ...previous,
                  dependencyViewport,
                }))
              }
              onUpstreamDepthChange={(dependencyUpstreamDepth) =>
                setSession((previous) => ({
                  ...previous,
                  dependencyUpstreamDepth,
                }))
              }
              onDownstreamDepthChange={(dependencyDownstreamDepth) =>
                setSession((previous) => ({
                  ...previous,
                  dependencyDownstreamDepth,
                }))
              }
            />
          )}

        {projection &&
          projection.nodes.length > 0 &&
          session.activeView === "replay" && (
            <ReplayView
              liveProjection={projection}
              connection={data.connection}
              refreshEpoch={data.refreshEpoch}
              session={session}
              setSession={setSession}
            />
          )}

        {session.activeView !== "replay" && (
          <Inspector
            node={session.inspectorOpen ? selectedNode : null}
            detailState={detail}
            dependencies={projection?.dependencies ?? []}
            nodes={projection?.nodes ?? []}
            annotation={
              selectedNode ? session.annotations[selectedNode.id] ?? "" : ""
            }
            onAnnotationChange={(annotation) => {
              if (!selectedNode) {
                return;
              }
              setSession((previous) => ({
                ...previous,
                annotations: {
                  ...previous.annotations,
                  [selectedNode.id]: annotation,
                },
              }));
            }}
            onSelectRelated={(id) => {
              navigateToBranch(id);
            }}
            onClose={() =>
              setSession((previous) => ({
                ...previous,
                selected:
                  previous.activeView === "dependency"
                    ? previous.selected
                    : "",
                inspectorOpen: false,
              }))
            }
          />
        )}

        <CanvasSettingsPanel
          open={settingsOpen}
          settings={session.settings}
          onChange={(settings) =>
            setSession((previous) => ({ ...previous, settings }))
          }
          onClose={() => setSettingsOpen(false)}
        />
      </main>
    </div>
  );
}
