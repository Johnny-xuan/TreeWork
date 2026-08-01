import { useMemo, useRef } from "react";
import type {
  KeyboardEvent as ReactKeyboardEvent,
  Ref,
} from "react";
import type { ProjectMapNode } from "../../data/types";
import {
  DEPENDENCY_COLUMN_STEP,
  type DependencyLayout,
} from "../../layout/dependencyLayout";
import {
  MAP_NODE_HEIGHT,
  MAP_NODE_WIDTH,
} from "../../layout/mapLayout";
import { BranchNode } from "../shared/BranchNode";

interface DependencySceneProps {
  layout: DependencyLayout;
  nodes: readonly ProjectMapNode[];
  selected: string;
  dimmed: ReadonlySet<string>;
  matches: ReadonlySet<string>;
  contentRef: Ref<SVGGElement>;
  onSelect: (id: string) => void;
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

export function DependencyScene({
  layout,
  nodes,
  selected,
  dimmed,
  matches,
  contentRef,
  onSelect,
}: DependencySceneProps) {
  const byId = useMemo(
    () => new Map(nodes.map((node) => [node.id, node])),
    [nodes],
  );
  const elementRefs = useRef(new Map<string, SVGGElement>());
  const causalBottom = Math.max(
    ...[...layout.positions.values()]
      .filter((position) => position.role !== "parallel")
      .map((position) => position.y + MAP_NODE_HEIGHT),
  );
  const parallelTop = Math.min(
    ...[...layout.positions.values()]
      .filter((position) => position.role === "parallel")
      .map((position) => position.y),
  );
  const guideTop = layout.minY - 54;
  const guideBottom = causalBottom + 54;
  const focusTitle = byId.get(layout.focusId)?.title ?? layout.focusId;

  const focusNode = (id: string | undefined) => {
    if (id) {
      elementRefs.current.get(id)?.focus();
    }
  };

  const onNodeKeyDown = (
    event: ReactKeyboardEvent<SVGGElement>,
    node: ProjectMapNode,
  ) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onSelect(node.id);
      return;
    }
    const position = layout.positions.get(node.id);
    if (!position) {
      return;
    }
    if (
      event.key === "ArrowLeft" ||
      event.key === "ArrowRight"
    ) {
      event.preventDefault();
      const direction = event.key === "ArrowLeft" ? -1 : 1;
      const candidate = [...layout.positions.values()]
        .filter((item) =>
          direction < 0 ? item.x < position.x : item.x > position.x,
        )
        .sort(
          (left, right) =>
            Math.abs(left.x - position.x) -
              Math.abs(right.x - position.x) ||
            Math.abs(left.y - position.y) -
              Math.abs(right.y - position.y) ||
            left.id.localeCompare(right.id),
        )[0];
      focusNode(candidate?.id);
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
    <g
      ref={contentRef}
      className="map-content dependency-content"
      data-testid="dependency-content"
    >
      <defs>
        <marker
          id="dependency-arrow-satisfied"
          markerWidth="7"
          markerHeight="7"
          refX="6"
          refY="3.5"
          orient="auto"
          markerUnits="strokeWidth"
        >
          <path d="M 0 0 L 7 3.5 L 0 7 z" />
        </marker>
        <marker
          id="dependency-arrow-unsatisfied"
          markerWidth="7"
          markerHeight="7"
          refX="6"
          refY="3.5"
          orient="auto"
          markerUnits="strokeWidth"
        >
          <path d="M 0 0 L 7 3.5 L 0 7 z" />
        </marker>
      </defs>
      <g className="dependency-guides" aria-hidden="true">
        {Array.from(
          {
            length:
              layout.visibleUpstreamDepth +
              layout.visibleDownstreamDepth +
              1,
          },
          (_, index) => index - layout.visibleUpstreamDepth,
        ).map((depth) => (
          <line
            key={depth}
            className={depth === 0 ? "is-focus-guide" : ""}
            x1={depth * DEPENDENCY_COLUMN_STEP - 28}
            y1={guideTop}
            x2={depth * DEPENDENCY_COLUMN_STEP - 28}
            y2={guideBottom}
          />
        ))}
        <text
          x={-layout.visibleUpstreamDepth * DEPENDENCY_COLUMN_STEP + 16}
          y={guideTop + 2}
        >
          Prerequisites
        </text>
        <text x={16} y={guideTop + 2}>
          Focused branch
        </text>
        <text
          x={Math.max(1, layout.visibleDownstreamDepth) * DEPENDENCY_COLUMN_STEP + 16}
          y={guideTop + 2}
        >
          Dependents
        </text>
      </g>

      <g className="dependency-connectors" aria-label="Dependency relations">
        {layout.edges.map((edge) => {
          const from = layout.positions.get(edge.from)!;
          const to = layout.positions.get(edge.to)!;
          const fromNode = byId.get(edge.from);
          const toNode = byId.get(edge.to);
          const startX = from.x + MAP_NODE_WIDTH;
          const startY = from.y + MAP_NODE_HEIGHT / 2;
          const endX = to.x;
          const endY = to.y + MAP_NODE_HEIGHT / 2;
          return (
            <g
              key={`${edge.from}:${edge.to}`}
              data-edge={`${edge.from}:${edge.to}`}
              data-satisfied={edge.satisfied}
              aria-label={`${fromNode?.title ?? edge.from} is a prerequisite for ${toNode?.title ?? edge.to}, ${edge.satisfied ? "satisfied" : "unsatisfied"}`}
            >
              <path
                className={edge.satisfied ? "is-satisfied" : "is-unsatisfied"}
                d={connectorPath(startX, startY, endX, endY)}
                markerEnd={`url(#dependency-arrow-${edge.satisfied ? "satisfied" : "unsatisfied"})`}
              />
              <text
                className={edge.satisfied ? "is-satisfied" : "is-unsatisfied"}
                x={(startX + endX) / 2}
                y={(startY + endY) / 2 - 7}
                textAnchor="middle"
              >
                {edge.satisfied ? "✓ satisfied" : "○ unsatisfied"}
              </text>
            </g>
          );
        })}
      </g>

      {layout.parallelCandidates.length > 0 && (
        <g className="parallel-lane-heading" aria-hidden="true">
          <line
            x1={layout.minX}
            y1={parallelTop - 76}
            x2={layout.maxX}
            y2={parallelTop - 76}
          />
          <text x={layout.minX} y={parallelTop - 48}>
            Ready for parallel work
          </text>
          <text className="parallel-disclaimer" x={layout.minX} y={parallelTop - 28}>
            Ready leaf branches with no dependency path to {focusTitle} · check shared files before assigning
          </text>
        </g>
      )}

      <g
        role="listbox"
        aria-label="Focused dependency branches"
        aria-multiselectable="false"
      >
        {layout.orderedIds.map((id) => {
          const node = byId.get(id);
          const position = layout.positions.get(id);
          if (!node || !position) {
            return null;
          }
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
              className={[
                position.role === "focus" ? "is-dependency-focus" : "",
                position.role === "parallel" ? "is-parallel-candidate" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              selected={selected === id}
              dimmed={dimmed.has(id)}
              matched={matches.has(id)}
              depth={position.distance}
              nodeRole={position.role}
              semanticRole="option"
              tabIndex={selected === id ? 0 : -1}
              onSelect={onSelect}
              onKeyDown={onNodeKeyDown}
            />
          );
        })}
      </g>
    </g>
  );
}
