import type {
  CSSProperties,
  KeyboardEvent as ReactKeyboardEvent,
  ReactNode,
} from "react";
import {
  statusPresentation,
  verificationPresentation,
} from "../../app/status";
import type { ProjectMapNode } from "../../data/types";
import {
  MAP_NODE_HEIGHT,
  MAP_NODE_WIDTH,
} from "../../layout/mapLayout";
import { truncate, wrapPurpose } from "../map/text";

export interface ReplayNodeMotion {
  key: number;
  kind: "enter" | "move" | "change" | "exit";
  deltaX: number;
  deltaY: number;
  durationMs: number;
}

interface BranchNodeProps {
  node: ProjectMapNode;
  x: number;
  y: number;
  className?: string;
  selected: boolean;
  dimmed?: boolean;
  matched?: boolean;
  depth?: number;
  nodeRole?: string;
  semanticRole: "treeitem" | "option";
  ariaLevel?: number;
  ariaExpanded?: boolean;
  tabIndex: number;
  summary?: string;
  accessory?: ReactNode;
  motion?: ReplayNodeMotion;
  elementRef?: (element: SVGGElement | null) => void;
  onSelect: (id: string) => void;
  onDoubleClick?: (id: string) => void;
  onKeyDown: (
    event: ReactKeyboardEvent<SVGGElement>,
    node: ProjectMapNode,
  ) => void;
}

export function BranchNode({
  node,
  x,
  y,
  className = "",
  selected,
  dimmed = false,
  matched = false,
  depth,
  nodeRole,
  semanticRole,
  ariaLevel,
  ariaExpanded,
  tabIndex,
  summary = "",
  accessory,
  motion,
  elementRef,
  onSelect,
  onDoubleClick,
  onKeyDown,
}: BranchNodeProps) {
  const status = statusPresentation(node);
  const verification = verificationPresentation(node.verification);
  const purposeLines = wrapPurpose(node.purpose);
  const classNames = [
    "branch-node",
    status.className,
    className,
    selected ? "is-selected" : "",
    dimmed ? "is-dimmed" : "",
    matched ? "is-match" : "",
  ]
    .filter(Boolean)
    .join(" ");
  const motionStyle = motion
    ? ({
        "--tw-replay-x": `${motion.deltaX}px`,
        "--tw-replay-y": `${motion.deltaY}px`,
        "--tw-replay-duration": `${motion.durationMs}ms`,
      } as CSSProperties)
    : undefined;

  return (
    <g
      ref={elementRef}
      className={classNames}
      transform={`translate(${x} ${y})`}
      data-node-id={node.id}
      data-depth={depth}
      data-node-role={nodeRole}
      data-status={node.status}
      data-readiness={node.readiness}
      data-verification={node.verification}
      role={semanticRole}
      aria-level={ariaLevel}
      aria-selected={selected}
      aria-expanded={ariaExpanded}
      aria-label={`${node.title}, ${status.label}, ${verification.label}`}
      tabIndex={tabIndex}
      onClick={(event) => {
        event.stopPropagation();
        onSelect(node.id);
      }}
      onDoubleClick={(event) => {
        if (!onDoubleClick) {
          return;
        }
        event.stopPropagation();
        onDoubleClick(node.id);
      }}
      onKeyDown={(event) => onKeyDown(event, node)}
    >
      <g
        key={motion?.key ?? 0}
        className={[
          "branch-node-visual",
          motion ? `is-replay-${motion.kind}` : "",
        ]
          .filter(Boolean)
          .join(" ")}
        style={motionStyle}
      >
        <rect
          className="node-hit-area"
          width={MAP_NODE_WIDTH}
          height={MAP_NODE_HEIGHT}
        />
        <rect
          className="node-selection"
          width={MAP_NODE_WIDTH}
          height={MAP_NODE_HEIGHT}
          rx={5}
        />
        <rect
          className="status-rail"
          x={0}
          y={8}
          width={3}
          height={MAP_NODE_HEIGHT - 16}
        />

        <text className="node-meta" x={17} y={14}>
          <tspan className="status-symbol">{status.symbol}</tspan>
          <tspan dx={7}>{status.label}</tspan>
          <tspan dx={9} className="branch-id">
            {truncate(node.id, 22)}
          </tspan>
        </text>
        <text
          className={`verification-mark ${verification.className}`}
          x={accessory ? MAP_NODE_WIDTH - 39 : MAP_NODE_WIDTH - 15}
          y={14}
          textAnchor="end"
          aria-label={verification.label}
        >
          {verification.symbol}
        </text>
        {accessory}

        <text className="node-title" x={17} y={39}>
          {truncate(node.title, 27)}
        </text>
        {purposeLines.map((line, index) => (
          <text
            key={`${node.id}:purpose:${index}`}
            className="node-purpose"
            x={17}
            y={59 + index * 14}
          >
            {line}
          </text>
        ))}
        {summary && <title>{`${node.title}: ${summary}`}</title>}
      </g>
    </g>
  );
}
