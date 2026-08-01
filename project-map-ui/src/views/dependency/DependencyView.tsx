import {
  Focus,
  Minus,
  Plus,
  Scan,
  ZoomIn,
  ZoomOut,
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
import { layoutDependency } from "../../layout/dependencyLayout";
import {
  MAP_NODE_HEIGHT,
  MAP_NODE_WIDTH,
} from "../../layout/mapLayout";
import type {
  CanvasSettings,
  ViewportTransform,
} from "../../state/session";
import { DependencyScene } from "./DependencyScene";

const MIN_SCALE = 0.3;
const MAX_SCALE = 1.8;
const MOBILE_READABLE_SCALE = 0.58;

interface DependencyViewProps {
  projection: ProjectMapProjection;
  focusId: string;
  query: string;
  statusFilter: LifecycleStatus | "all";
  upstreamDepth: number;
  downstreamDepth: number;
  viewport: ViewportTransform | null;
  settings: CanvasSettings;
  locateNonce: number;
  fitNonce: number;
  onSelect: (id: string) => void;
  onViewportChange: (viewport: ViewportTransform) => void;
  onUpstreamDepthChange: (depth: number) => void;
  onDownstreamDepthChange: (depth: number) => void;
}

function clamp(value: number, minimum: number, maximum: number): number {
  return Math.max(minimum, Math.min(maximum, value));
}

export function DependencyView({
  projection,
  focusId,
  query,
  statusFilter,
  upstreamDepth,
  downstreamDepth,
  viewport,
  settings,
  locateNonce,
  fitNonce,
  onSelect,
  onViewportChange,
  onUpstreamDepthChange,
  onDownstreamDepthChange,
}: DependencyViewProps) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<SVGGElement>(null);
  const dragging = useRef<{
    pointerId: number;
    originX: number;
    originY: number;
    startX: number;
    startY: number;
  } | null>(null);
  const transform = useRef<ViewportTransform>(
    viewport ?? { x: 0, y: 0, scale: 1 },
  );
  const initialViewportApplied = useRef(false);
  const previousFocusId = useRef(focusId);
  const commitTimer = useRef(0);
  const [zoomPercent, setZoomPercent] = useState(
    Math.round(transform.current.scale * 100),
  );

  const layout = useMemo(
    () =>
      layoutDependency(
        projection.nodes,
        projection.dependencies,
        focusId,
        upstreamDepth,
        downstreamDepth,
      ),
    [
      downstreamDepth,
      focusId,
      projection.dependencies,
      projection.nodes,
      upstreamDepth,
    ],
  );
  const byId = useMemo(
    () => new Map(projection.nodes.map((node) => [node.id, node])),
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

  const applyTransform = (
    next: ViewportTransform,
    commit = false,
  ) => {
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

  const centerFocus = () => {
    const surface = surfaceRef.current;
    const position = layout.positions.get(focusId);
    if (!surface || !position) {
      return;
    }
    const bounds = surface.getBoundingClientRect();
    const scale =
      bounds.width <= 760
        ? MOBILE_READABLE_SCALE
        : transform.current.scale;
    applyTransform(
      {
        x:
          bounds.width / 2 -
          (position.x + MAP_NODE_WIDTH / 2) * scale,
        y:
          bounds.height * 0.42 -
          (position.y + MAP_NODE_HEIGHT / 2) * scale,
        scale,
      },
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
        (bounds.width - 64) / layout.width,
        (bounds.height - 104) / layout.height,
      ),
      MIN_SCALE,
      1.05,
    );
    applyTransform(
      {
        x:
          (bounds.width - (layout.maxX - layout.minX) * scale) / 2 -
          layout.minX * scale,
        y:
          (bounds.height - (layout.maxY - layout.minY) * scale) / 2 -
          layout.minY * scale,
        scale,
      },
      true,
    );
  };

  useLayoutEffect(() => {
    applyTransform(transform.current);
    if (initialViewportApplied.current) {
      return;
    }
    initialViewportApplied.current = true;
    if (viewport) {
      applyTransform(viewport);
      const surfaceWidth =
        surfaceRef.current?.getBoundingClientRect().width;
      if (
        surfaceWidth !== undefined &&
        surfaceWidth <= 760 &&
        viewport.scale < MOBILE_READABLE_SCALE
      ) {
        window.requestAnimationFrame(centerFocus);
      }
    } else {
      window.requestAnimationFrame(centerFocus);
    }
  }, [viewport]);

  useLayoutEffect(() => {
    if (previousFocusId.current === focusId) {
      return;
    }
    previousFocusId.current = focusId;
    window.requestAnimationFrame(centerFocus);
    // Focus replacement is an explicit viewport command.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [focusId]);

  useEffect(() => {
    if (locateNonce > 0) {
      centerFocus();
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
      const nextScale = clamp(
        transform.current.scale *
          Math.exp(
            -event.deltaY *
              0.0022 *
              settings.zoomSensitivity,
          ),
        MIN_SCALE,
        MAX_SCALE,
      );
      const ratio = nextScale / transform.current.scale;
      applyTransform(
        {
          x:
            pointerX -
            (pointerX - transform.current.x) * ratio,
          y:
            pointerY -
            (pointerY - transform.current.y) * ratio,
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
        x:
          centerX -
          (centerX - transform.current.x) * ratio,
        y:
          centerY -
          (centerY - transform.current.y) * ratio,
        scale: nextScale,
      },
      true,
    );
  };

  const waitingNames = layout.unsatisfiedPrerequisites.map(
    (id) => byId.get(id)?.title ?? id,
  );

  return (
    <section
      className="map-view dependency-view"
      aria-label={`Dependencies for ${byId.get(focusId)?.title ?? focusId}`}
    >
      <div
        id="dependencySurface"
        ref={surfaceRef}
        className="map-surface dependency-surface"
        data-testid="dependency-surface"
        onPointerDown={(event) => {
          if (
            event.button !== 0 ||
            (event.target as Element).closest(
              ".branch-node, .canvas-tools, .dependency-depth-controls",
            )
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
      >
        <svg
          id="dependencySvg"
          className="map-svg dependency-svg"
          width="100%"
          height="100%"
          role="img"
          aria-label={`${layout.causalIds.size} branches in the dependency chain and ${layout.parallelCandidates.length} branches ready for parallel work`}
        >
          <DependencyScene
            layout={layout}
            nodes={projection.nodes}
            selected={focusId}
            dimmed={dimmed}
            matches={matches}
            contentRef={contentRef}
            onSelect={onSelect}
          />
        </svg>

        <div
          className="dependency-depth-controls"
          aria-label="Dependency depth controls"
        >
          <div>
            <span>Prerequisites</span>
            <button
              type="button"
              aria-label="Reduce upstream depth"
              title="Reduce upstream depth"
              disabled={upstreamDepth <= 1}
              onClick={() =>
                onUpstreamDepthChange(Math.max(1, upstreamDepth - 1))
              }
            >
              <Minus size={14} />
            </button>
            <output aria-label="Visible upstream depth">
              {layout.visibleUpstreamDepth}
            </output>
            <button
              type="button"
              aria-label="Expand upstream depth"
              title="Expand upstream depth"
              disabled={
                layout.maxUpstreamDepth === 0 ||
                upstreamDepth >= layout.maxUpstreamDepth
              }
              onClick={() =>
                onUpstreamDepthChange(upstreamDepth + 1)
              }
            >
              <Plus size={14} />
            </button>
          </div>
          <div>
            <span>Dependents</span>
            <button
              type="button"
              aria-label="Reduce downstream depth"
              title="Reduce downstream depth"
              disabled={downstreamDepth <= 1}
              onClick={() =>
                onDownstreamDepthChange(
                  Math.max(1, downstreamDepth - 1),
                )
              }
            >
              <Minus size={14} />
            </button>
            <output aria-label="Visible downstream depth">
              {layout.visibleDownstreamDepth}
            </output>
            <button
              type="button"
              aria-label="Expand downstream depth"
              title="Expand downstream depth"
              disabled={
                layout.maxDownstreamDepth === 0 ||
                downstreamDepth >= layout.maxDownstreamDepth
              }
              onClick={() =>
                onDownstreamDepthChange(downstreamDepth + 1)
              }
            >
              <Plus size={14} />
            </button>
          </div>
        </div>

        {waitingNames.length > 0 && (
          <div className="dependency-waiting" role="status">
            <strong>Waiting on direct prerequisites</strong>
            <span>{waitingNames.join(" · ")}</span>
          </div>
        )}

        {layout.parallelCandidates.length === 0 && (
          <div className="dependency-parallel-empty" role="status">
            No other ready leaf branches can run in parallel
          </div>
        )}

        <nav className="canvas-tools" aria-label="Dependency canvas controls">
          <button
            type="button"
            aria-label="Zoom in"
            title="Zoom in"
            onClick={() => zoomAtCenter(1.16)}
          >
            <ZoomIn size={16} />
          </button>
          <button
            type="button"
            aria-label="Zoom out"
            title="Zoom out"
            onClick={() => zoomAtCenter(1 / 1.16)}
          >
            <ZoomOut size={16} />
          </button>
          <span aria-hidden="true" />
          <button
            type="button"
            aria-label="Fit dependency view"
            title="Fit dependency view (F)"
            onClick={fit}
          >
            <Scan size={16} />
          </button>
          <button
            type="button"
            aria-label="Center focused branch"
            title="Center focused branch"
            onClick={centerFocus}
          >
            <Focus size={16} />
          </button>
        </nav>

        <footer className="map-statusbar" aria-live="polite">
          <span>
            {layout.causalIds.size} causal ·{" "}
            {layout.parallelCandidates.length} parallel-ready
          </span>
          <span>{zoomPercent}%</span>
          <span>Revision {projection.tree_revision}</span>
        </footer>
      </div>
    </section>
  );
}
