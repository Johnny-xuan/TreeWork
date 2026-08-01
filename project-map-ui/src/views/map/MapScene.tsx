import { useMemo, useRef } from "react";
import type { KeyboardEvent as ReactKeyboardEvent, Ref } from "react";
import type { ProjectMapNode } from "../../data/types";
import {
  MAP_COLUMN_STEP,
  MAP_NODE_HEIGHT,
  MAP_NODE_WIDTH,
  MAP_PADDING_X,
  type MapLayout,
} from "../../layout/mapLayout";
import {
  BranchNode,
  type ReplayNodeMotion,
} from "../shared/BranchNode";

interface MapSceneProps {
  layout: MapLayout;
  nodes: ProjectMapNode[];
  route: ReadonlySet<string>;
  selected: string;
  dimmed: ReadonlySet<string>;
  matches: ReadonlySet<string>;
  collapsed: ReadonlySet<string>;
  subtreeCounts: ReadonlyMap<string, Record<string, number>>;
  contentRef: Ref<SVGGElement>;
  previousLayout?: MapLayout | null;
  previousNodes?: readonly ProjectMapNode[];
  previousRoute?: ReadonlySet<string>;
  transitionKey?: number;
  transitionDurationMs?: number;
  onSelect: (id: string) => void;
  onToggleCollapse: (id: string) => void;
}

function connectorPath(
  fromX: number,
  fromY: number,
  toX: number,
  toY: number,
): string {
  const middle = (fromX + toX) / 2;
  return `M ${fromX} ${fromY} C ${middle} ${fromY}, ${middle} ${toY}, ${toX} ${toY}`;
}

function sameNodePresentation(
  left: ProjectMapNode,
  right: ProjectMapNode,
): boolean {
  return (
    left.parent === right.parent &&
    left.order === right.order &&
    left.title === right.title &&
    left.purpose === right.purpose &&
    left.spec === right.spec &&
    left.status === right.status &&
    left.verification === right.verification &&
    left.status_reason === right.status_reason &&
    left.is_current === right.is_current &&
    left.readiness === right.readiness &&
    left.depends_on.join("\u0000") === right.depends_on.join("\u0000")
  );
}

export function MapScene({
  layout,
  nodes,
  route,
  selected,
  dimmed,
  matches,
  collapsed,
  subtreeCounts,
  contentRef,
  previousLayout = null,
  previousNodes = [],
  previousRoute = new Set<string>(),
  transitionKey,
  transitionDurationMs = 320,
  onSelect,
  onToggleCollapse,
}: MapSceneProps) {
  const nodeById = useMemo(
    () => new Map(nodes.map((node) => [node.id, node])),
    [nodes],
  );
  const previousNodeById = useMemo(
    () => new Map(previousNodes.map((node) => [node.id, node])),
    [previousNodes],
  );
  const elementRefs = useRef(new Map<string, SVGGElement>());
  const motionFor = (
    node: ProjectMapNode,
    x: number,
    y: number,
  ): ReplayNodeMotion | undefined => {
    if (transitionKey === undefined || !previousLayout) {
      return undefined;
    }
    const previousNode = previousNodeById.get(node.id);
    const previousPosition = previousLayout.positions.get(node.id);
    if (!previousNode || !previousPosition) {
      const parentPosition =
        previousLayout.positions.get(node.parent) ??
        layout.positions.get(node.parent);
      return {
        key: transitionKey,
        kind: "enter",
        deltaX: parentPosition ? parentPosition.x - x : 0,
        deltaY: parentPosition ? parentPosition.y - y : 0,
        durationMs: transitionDurationMs,
      };
    }
    const deltaX = previousPosition.x - x;
    const deltaY = previousPosition.y - y;
    if (deltaX || deltaY) {
      return {
        key: transitionKey,
        kind: "move",
        deltaX,
        deltaY,
        durationMs: transitionDurationMs,
      };
    }
    if (!sameNodePresentation(previousNode, node)) {
      return {
        key: transitionKey,
        kind: "change",
        deltaX: 0,
        deltaY: 0,
        durationMs: transitionDurationMs,
      };
    }
    return undefined;
  };

  const focusNode = (id: string | undefined) => {
    if (id) {
      elementRefs.current.get(id)?.focus();
    }
  };

  const onNodeKeyDown = (
    event: ReactKeyboardEvent<SVGGElement>,
    node: ProjectMapNode,
  ) => {
    const position = layout.positions.get(node.id);
    if (!position) {
      return;
    }
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelect(node.id);
      return;
    }
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      focusNode(node.parent);
      return;
    }
    if (event.key === "ArrowRight") {
      event.preventDefault();
      focusNode(
        nodes
          .filter((candidate) => candidate.parent === node.id)
          .sort((left, right) => left.order - right.order)[0]?.id,
      );
      return;
    }
    const index = layout.orderedIds.indexOf(node.id);
    if (event.key === "ArrowUp" && index > 0) {
      event.preventDefault();
      focusNode(layout.orderedIds[index - 1]);
    } else if (
      event.key === "ArrowDown" &&
      index < layout.orderedIds.length - 1
    ) {
      event.preventDefault();
      focusNode(layout.orderedIds[index + 1]);
    }
  };

  return (
    <g ref={contentRef} className="map-content" data-testid="map-content">
      <g className="depth-guides" aria-hidden="true">
        {Array.from({ length: layout.depthCount }, (_, depth) => (
          <line
            key={depth}
            x1={MAP_PADDING_X - 28 + depth * MAP_COLUMN_STEP}
            y1={20}
            x2={MAP_PADDING_X - 28 + depth * MAP_COLUMN_STEP}
            y2={Math.max(layout.height, 300)}
          />
        ))}
      </g>

      <g className="parent-connectors" aria-hidden="true">
        {layout.links.map((link) => {
          const from = layout.positions.get(link.from)!;
          const to = layout.positions.get(link.to)!;
          const isRoute = route.has(link.from) && route.has(link.to);
          return (
            <path
              key={`${link.from}:${link.to}`}
              data-edge={`${link.from}:${link.to}`}
              className={isRoute ? "is-route" : ""}
              d={connectorPath(
                from.x + MAP_NODE_WIDTH,
                from.y + MAP_NODE_HEIGHT / 2,
                to.x,
                to.y + MAP_NODE_HEIGHT / 2,
              )}
            />
          );
        })}
      </g>

      {previousLayout && transitionKey !== undefined && (
        <g className="replay-exiting-nodes" aria-hidden="true">
          {previousLayout.orderedIds
            .filter((id) => !nodeById.has(id))
            .map((id) => {
              const node = previousNodeById.get(id);
              const position = previousLayout.positions.get(id);
              if (!node || !position) {
                return null;
              }
              const target =
                layout.positions.get(node.parent) ??
                previousLayout.positions.get(node.parent);
              return (
                <BranchNode
                  key={`${id}:${transitionKey}`}
                  node={node}
                  x={position.x}
                  y={position.y}
                  className={previousRoute.has(id) ? "is-route" : ""}
                  selected={false}
                  depth={position.depth}
                  semanticRole="treeitem"
                  ariaLevel={position.depth + 1}
                  tabIndex={-1}
                  motion={{
                    key: transitionKey,
                    kind: "exit",
                    deltaX: target ? target.x - position.x : 0,
                    deltaY: target ? target.y - position.y : 0,
                    durationMs: transitionDurationMs,
                  }}
                  onSelect={() => undefined}
                  onKeyDown={() => undefined}
                />
              );
            })}
        </g>
      )}

      <g role="tree" aria-label="TreeWork branch hierarchy">
        {layout.orderedIds.map((id) => {
          const node = nodeById.get(id)!;
          const position = layout.positions.get(id)!;
          const childCount = node.child_count;
          const hidden = position.hiddenDescendants;
          const counts = subtreeCounts.get(id) ?? {};
          const summary = hidden
            ? `${hidden} hidden · ${counts.in_progress ?? 0} active · ${counts.pending ?? 0} pending`
            : "";
          return (
            <BranchNode
              key={id}
              elementRef={(element) => {
                if (element) {
                  elementRefs.current.set(id, element);
                } else {
                  elementRefs.current.delete(id);
                }
              }}
              node={node}
              x={position.x}
              y={position.y}
              className={route.has(id) ? "is-route" : ""}
              selected={selected === id}
              dimmed={dimmed.has(id)}
              matched={matches.has(id)}
              depth={position.depth}
              semanticRole="treeitem"
              ariaLevel={position.depth + 1}
              ariaExpanded={childCount ? !collapsed.has(id) : undefined}
              tabIndex={selected === id || (!selected && node.is_current) ? 0 : -1}
              summary={summary}
              motion={motionFor(node, position.x, position.y)}
              onSelect={onSelect}
              onDoubleClick={childCount ? onToggleCollapse : undefined}
              onKeyDown={onNodeKeyDown}
              accessory={
                childCount > 0 ? (
                <g
                  className="collapse-control"
                  role="button"
                  tabIndex={0}
                  aria-label={
                    collapsed.has(id)
                      ? `Expand ${node.title}`
                      : `Collapse ${node.title}`
                  }
                  onClick={(event) => {
                    event.stopPropagation();
                    onToggleCollapse(id);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter" || event.key === " ") {
                      event.preventDefault();
                      event.stopPropagation();
                      onToggleCollapse(id);
                    }
                  }}
                >
                  <rect x={MAP_NODE_WIDTH - 31} y={1} width={27} height={24} rx={4} />
                  <text x={MAP_NODE_WIDTH - 17} y={16} textAnchor="middle">
                    {collapsed.has(id) ? "›" : "⌄"}
                  </text>
                </g>
                ) : null
              }
            />
          );
        })}
      </g>
    </g>
  );
}
