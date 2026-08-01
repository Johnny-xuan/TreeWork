import {
  LocateFixed,
  Minus,
  Plus,
  Scan,
} from "lucide-react";
import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import type {
  LifecycleStatus,
  ProjectMapProjection,
} from "../../data/types";
import {
  currentRoute,
  layoutMap,
  subtreeStatusCounts,
} from "../../layout/mapLayout";
import type {
  CanvasSettings,
  ViewportTransform,
} from "../../state/session";
import { MapScene } from "./MapScene";
import {
  shouldRecoverNarrowMapViewport,
  viewportForMapTarget,
} from "./viewport";

const MIN_SCALE = 0.42;
const MAX_SCALE = 1.8;

interface MapViewProps {
  projection: ProjectMapProjection;
  selected: string;
  query: string;
  statusFilter: LifecycleStatus | "all";
  collapsed: string[];
  viewport: ViewportTransform | null;
  settings: CanvasSettings;
  locateNonce: number;
  fitNonce: number;
  viewportFocusId?: string;
  recoverNarrowViewport?: boolean;
  onNarrowViewportRecoveryHandled?: () => void;
  transitionFrom?: ProjectMapProjection | null;
  transitionKey?: number;
  transitionDurationMs?: number;
  onSelect: (id: string) => void;
  onClearSelection: () => void;
  onCollapsedChange: (collapsed: string[]) => void;
  onViewportChange: (viewport: ViewportTransform) => void;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, value));
}

export function MapView({
  projection,
  selected,
  query,
  statusFilter,
  collapsed,
  viewport,
  settings,
  locateNonce,
  fitNonce,
  viewportFocusId = "",
  recoverNarrowViewport = false,
  onNarrowViewportRecoveryHandled,
  transitionFrom = null,
  transitionKey,
  transitionDurationMs,
  onSelect,
  onClearSelection,
  onCollapsedChange,
  onViewportChange,
}: MapViewProps) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<SVGGElement>(null);
  const dragging = useRef<{
    pointerId: number;
    originX: number;
    originY: number;
    startX: number;
    startY: number;
  } | null>(null);
  const initialViewportApplied = useRef(false);
  const previousLayout = useRef<ReturnType<typeof layoutMap> | null>(null);
  const previousRevision = useRef(projection.tree_revision);
  const transform = useRef<ViewportTransform>(
    viewport ?? { x: 38, y: 38, scale: 1 },
  );
  const commitTimer = useRef(0);
  const [zoomPercent, setZoomPercent] = useState(
    Math.round(transform.current.scale * 100),
  );

  const collapsedKey = [...collapsed].sort().join("\u0000");
  const collapsedSet = useMemo(
    () => new Set(collapsed),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [collapsedKey],
  );
  const layout = useMemo(
    () => layoutMap(projection.nodes, collapsedSet),
    // Topology and explicit collapse state are the only coordinate inputs.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [projection.tree_revision, collapsedKey],
  );
  const transitionLayout = useMemo(
    () =>
      transitionFrom
        ? layoutMap(transitionFrom.nodes, collapsedSet)
        : null,
    // The transition key identifies a particular accepted state change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [transitionFrom?.tree_revision, transitionKey, collapsedKey],
  );
  const route = useMemo(
    () => currentRoute(projection.nodes, projection.project.current_branch),
    [projection.nodes, projection.project.current_branch],
  );
  const transitionRoute = useMemo(
    () =>
      transitionFrom
        ? currentRoute(
            transitionFrom.nodes,
            transitionFrom.project.current_branch,
          )
        : new Set<string>(),
    [transitionFrom, transitionKey],
  );
  const subtreeCounts = useMemo(
    () => subtreeStatusCounts(projection.nodes),
    [projection.nodes],
  );
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matches = useMemo(() => {
    if (!normalizedQuery) {
      return new Set<string>();
    }
    return new Set(
      projection.nodes
        .filter((node) =>
          [node.title, node.id, node.purpose]
            .join(" ")
            .toLocaleLowerCase()
            .includes(normalizedQuery),
        )
        .map((node) => node.id),
    );
  }, [normalizedQuery, projection.nodes]);
  const dimmed = useMemo(
    () =>
      new Set(
        projection.nodes
          .filter(
            (node) =>
              (normalizedQuery && !matches.has(node.id)) ||
              (statusFilter !== "all" && node.status !== statusFilter),
          )
          .map((node) => node.id),
      ),
    [matches, normalizedQuery, projection.nodes, statusFilter],
  );
  const nodeById = useMemo(
    () => new Map(projection.nodes.map((node) => [node.id, node])),
    [projection.nodes],
  );

  const visibleFocusId = (preferredId: string): string => {
    let cursor = nodeById.get(preferredId);
    const visited = new Set<string>();
    while (cursor && !visited.has(cursor.id)) {
      if (layout.positions.has(cursor.id)) {
        return cursor.id;
      }
      visited.add(cursor.id);
      cursor = cursor.parent ? nodeById.get(cursor.parent) : undefined;
    }
    if (layout.positions.has(projection.project.current_branch)) {
      return projection.project.current_branch;
    }
    return layout.orderedIds[0] ?? "";
  };

  const applyTransform = (next: ViewportTransform, commit = false) => {
    transform.current = {
      x: next.x,
      y: next.y,
      scale: clamp(next.scale, MIN_SCALE, MAX_SCALE),
    };
    contentRef.current?.setAttribute(
      "transform",
      `translate(${transform.current.x} ${transform.current.y}) scale(${transform.current.scale})`,
    );
    setZoomPercent(Math.round(transform.current.scale * 100));
    if (commit) {
      window.clearTimeout(commitTimer.current);
      commitTimer.current = window.setTimeout(
        () => onViewportChange(transform.current),
        80,
      );
    }
  };

  const locate = (id: string, useNarrowReadableScale = false) => {
    const surface = surfaceRef.current;
    const position = layout.positions.get(visibleFocusId(id));
    if (!surface || !position) {
      return;
    }
    const bounds = surface.getBoundingClientRect();
    applyTransform(
      viewportForMapTarget(
        transform.current,
        { width: bounds.width, height: bounds.height },
        position,
        useNarrowReadableScale,
      ),
      true,
    );
  };

  const fit = () => {
    const surface = surfaceRef.current;
    if (!surface || !layout.width || !layout.height) {
      return;
    }
    const bounds = surface.getBoundingClientRect();
    const scale = clamp(
      Math.min(
        (bounds.width - 56) / layout.width,
        (bounds.height - 56) / layout.height,
      ),
      MIN_SCALE,
      1.05,
    );
    applyTransform(
      {
        x: (bounds.width - layout.width * scale) / 2,
        y: (bounds.height - layout.height * scale) / 2,
        scale,
      },
      true,
    );
  };

  useLayoutEffect(() => {
    applyTransform(transform.current);
    if (!initialViewportApplied.current) {
      initialViewportApplied.current = true;
      const focusId = visibleFocusId(
        viewportFocusId || projection.project.current_branch,
      );
      if (viewport) {
        applyTransform(viewport);
      }
      if (recoverNarrowViewport) {
        window.requestAnimationFrame(() => {
          const surface = surfaceRef.current;
          const position = layout.positions.get(focusId);
          if (
            !viewport ||
            (surface &&
              position &&
              shouldRecoverNarrowMapViewport(
                viewport,
                {
                  width: surface.getBoundingClientRect().width,
                  height: surface.getBoundingClientRect().height,
                },
                position,
              ))
          ) {
            locate(focusId, true);
          }
          onNarrowViewportRecoveryHandled?.();
        });
      } else if (!viewport) {
        window.requestAnimationFrame(() =>
          locate(focusId, Boolean(viewportFocusId)),
        );
      }
      previousLayout.current = layout;
      return;
    }

    if (previousRevision.current !== projection.tree_revision) {
      const anchorId =
        selected && layout.positions.has(selected)
          ? selected
          : projection.project.current_branch;
      const before = previousLayout.current?.positions.get(anchorId);
      const after = layout.positions.get(anchorId);
      if (before && after) {
        applyTransform(
          {
            x:
              transform.current.x +
              (before.x - after.x) * transform.current.scale,
            y:
              transform.current.y +
              (before.y - after.y) * transform.current.scale,
            scale: transform.current.scale,
          },
          true,
        );
      }
      previousRevision.current = projection.tree_revision;
    }
    previousLayout.current = layout;
  }, [
    layout,
    onNarrowViewportRecoveryHandled,
    projection.project.current_branch,
    projection.tree_revision,
    recoverNarrowViewport,
    selected,
    viewport,
    viewportFocusId,
  ]);

  useEffect(() => {
    if (locateNonce > 0) {
      locate(selected || projection.project.current_branch);
    }
    // locateNonce is an explicit command signal.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [locateNonce]);

  useEffect(() => {
    if (fitNonce > 0) {
      fit();
    }
    // fitNonce is an explicit command signal.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fitNonce]);

  useEffect(
    () => () => {
      window.clearTimeout(commitTimer.current);
    },
    [],
  );

  useEffect(() => {
    const surface = surfaceRef.current;
    if (!surface) {
      return;
    }
    const onWheel = (event: WheelEvent) => {
      event.preventDefault();
      const shouldZoom =
        event.ctrlKey ||
        event.metaKey ||
        settings.wheelMode === "zoom";
      if (!shouldZoom) {
        applyTransform(
          {
            x:
              transform.current.x -
              event.deltaX * settings.panSensitivity,
            y:
              transform.current.y -
              event.deltaY * settings.panSensitivity,
            scale: transform.current.scale,
          },
          true,
        );
        return;
      }
      const bounds = surface.getBoundingClientRect();
      const pointerX = event.clientX - bounds.left;
      const pointerY = event.clientY - bounds.top;
      const direction = Math.exp(
        -event.deltaY * 0.0022 * settings.zoomSensitivity,
      );
      const nextScale = clamp(
        transform.current.scale * direction,
        MIN_SCALE,
        MAX_SCALE,
      );
      const ratio = nextScale / transform.current.scale;
      applyTransform(
        {
          x: pointerX - (pointerX - transform.current.x) * ratio,
          y: pointerY - (pointerY - transform.current.y) * ratio,
          scale: nextScale,
        },
        true,
      );
    };
    surface.addEventListener("wheel", onWheel, { passive: false });
    return () => surface.removeEventListener("wheel", onWheel);
    // Canvas settings are the only changing inputs for native wheel behavior.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    settings.panSensitivity,
    settings.wheelMode,
    settings.zoomSensitivity,
  ]);

  const toggleCollapse = (id: string) => {
    const next = new Set(collapsed);
    if (next.has(id)) {
      next.delete(id);
    } else {
      next.add(id);
    }
    onCollapsedChange([...next]);
  };

  const zoomAtCenter = (factor: number) => {
    const surface = surfaceRef.current;
    if (!surface) {
      return;
    }
    const bounds = surface.getBoundingClientRect();
    const centerX = bounds.width / 2;
    const centerY = bounds.height / 2;
    const nextScale = clamp(
      transform.current.scale * factor,
      MIN_SCALE,
      MAX_SCALE,
    );
    const ratio = nextScale / transform.current.scale;
    applyTransform(
      {
        x: centerX - (centerX - transform.current.x) * ratio,
        y: centerY - (centerY - transform.current.y) * ratio,
        scale: nextScale,
      },
      true,
    );
  };

  return (
    <section className="map-view" aria-label="Project branch map">
      <div
        id="mapSurface"
        ref={surfaceRef}
        className="map-surface"
        data-testid="map-surface"
        onPointerDown={(event) => {
          if (
            event.button !== 0 ||
            (event.target as Element).closest(".branch-node, .canvas-tools")
          ) {
            return;
          }
          event.currentTarget.setPointerCapture(event.pointerId);
          dragging.current = {
            pointerId: event.pointerId,
            originX: event.clientX,
            originY: event.clientY,
            startX: transform.current.x,
            startY: transform.current.y,
          };
          event.currentTarget.classList.add("is-dragging");
        }}
        onPointerMove={(event) => {
          const drag = dragging.current;
          if (!drag || drag.pointerId !== event.pointerId) {
            return;
          }
          applyTransform({
            x: drag.startX + event.clientX - drag.originX,
            y: drag.startY + event.clientY - drag.originY,
            scale: transform.current.scale,
          });
        }}
        onPointerUp={(event) => {
          if (dragging.current?.pointerId === event.pointerId) {
            dragging.current = null;
            event.currentTarget.classList.remove("is-dragging");
            onViewportChange(transform.current);
          }
        }}
        onClick={(event) => {
          if (
            event.target === event.currentTarget ||
            (event.target as Element).classList.contains("map-svg")
          ) {
            onClearSelection();
          }
        }}
      >
        <svg
          id="projectMapSvg"
          className="map-svg"
          width="100%"
          height="100%"
          role="img"
          aria-label={`${projection.nodes.length} TreeWork branches in ${layout.depthCount} depth columns`}
        >
          <MapScene
            layout={layout}
            nodes={projection.nodes}
            route={route}
            selected={selected}
            dimmed={dimmed}
            matches={matches}
            collapsed={collapsedSet}
            subtreeCounts={subtreeCounts}
            contentRef={contentRef}
            previousLayout={transitionLayout}
            previousNodes={transitionFrom?.nodes}
            previousRoute={transitionRoute}
            transitionKey={transitionKey}
            transitionDurationMs={transitionDurationMs}
            onSelect={onSelect}
            onToggleCollapse={toggleCollapse}
          />
        </svg>

        {normalizedQuery && matches.size === 0 && (
          <div className="map-empty" role="status">
            <strong>No matching branches</strong>
            <span>Change the search to restore the full context.</span>
          </div>
        )}

        <nav className="canvas-tools" aria-label="Canvas controls">
          <button
            type="button"
            aria-label="Zoom in"
            title="Zoom in"
            onClick={() => zoomAtCenter(1.16)}
          >
            <Plus size={16} />
          </button>
          <button
            type="button"
            aria-label="Zoom out"
            title="Zoom out"
            onClick={() => zoomAtCenter(1 / 1.16)}
          >
            <Minus size={16} />
          </button>
          <span aria-hidden="true" />
          <button
            type="button"
            aria-label="Fit tree"
            title="Fit tree (F)"
            onClick={fit}
          >
            <Scan size={16} />
          </button>
          <button
            type="button"
            aria-label={
              viewportFocusId
                ? "Locate Replay focus"
                : "Locate current branch"
            }
            title={
              viewportFocusId
                ? "Locate Replay focus"
                : "Locate current branch (L)"
            }
            onClick={() =>
              locate(
                viewportFocusId || projection.project.current_branch,
                Boolean(viewportFocusId),
              )
            }
          >
            <LocateFixed size={16} />
          </button>
        </nav>

        <footer className="map-statusbar" aria-live="polite">
          <span>
            {projection.nodes.length} branches · {layout.depthCount} depths
          </span>
          <span>{zoomPercent}%</span>
          <span>Revision {projection.tree_revision}</span>
        </footer>
      </div>
    </section>
  );
}
