import type {
  ProjectMapDependency,
  ProjectMapNode,
} from "../data/types";
import {
  MAP_NODE_HEIGHT,
  MAP_NODE_WIDTH,
} from "./mapLayout";

export const DEPENDENCY_COLUMN_STEP = 360;
export const DEPENDENCY_ROW_STEP = 112;
export const DEPENDENCY_SCENE_PADDING = 72;
export const DEPENDENCY_PARALLEL_GAP = 176;

export type DependencyNodeRole =
  | "upstream"
  | "focus"
  | "downstream"
  | "parallel";

export interface DependencyPosition {
  id: string;
  role: DependencyNodeRole;
  distance: number;
  x: number;
  y: number;
}

export interface DependencyEdge {
  from: string;
  to: string;
  satisfied: boolean;
}

export interface DependencyLayout {
  focusId: string;
  positions: Map<string, DependencyPosition>;
  orderedIds: string[];
  causalIds: Set<string>;
  edges: DependencyEdge[];
  parallelCandidates: string[];
  unsatisfiedPrerequisites: string[];
  maxUpstreamDepth: number;
  maxDownstreamDepth: number;
  visibleUpstreamDepth: number;
  visibleDownstreamDepth: number;
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
  width: number;
  height: number;
}

interface Neighbor {
  id: string;
}

function compareNodes(
  byId: ReadonlyMap<string, ProjectMapNode>,
  left: string,
  right: string,
): number {
  const leftNode = byId.get(left);
  const rightNode = byId.get(right);
  return (
    (leftNode?.order ?? Number.MAX_SAFE_INTEGER) -
      (rightNode?.order ?? Number.MAX_SAFE_INTEGER) ||
    left.localeCompare(right)
  );
}

function distancesFrom(
  focusId: string,
  index: ReadonlyMap<string, readonly Neighbor[]>,
  byId: ReadonlyMap<string, ProjectMapNode>,
): Map<string, number> {
  const distances = new Map<string, number>([[focusId, 0]]);
  const queue = [focusId];
  for (let cursor = 0; cursor < queue.length; cursor += 1) {
    const id = queue[cursor];
    const distance = distances.get(id)!;
    const neighbors = [...(index.get(id) ?? [])].sort((left, right) =>
      compareNodes(byId, left.id, right.id),
    );
    for (const neighbor of neighbors) {
      if (!byId.has(neighbor.id) || distances.has(neighbor.id)) {
        continue;
      }
      distances.set(neighbor.id, distance + 1);
      queue.push(neighbor.id);
    }
  }
  distances.delete(focusId);
  return distances;
}

function maximumDistance(distances: ReadonlyMap<string, number>): number {
  return Math.max(0, ...distances.values());
}

function normalizedDepth(value: number): number {
  return Math.max(1, Math.floor(Number.isFinite(value) ? value : 1));
}

function hierarchyRelations(
  focusId: string,
  nodes: readonly ProjectMapNode[],
  byId: ReadonlyMap<string, ProjectMapNode>,
): Set<string> {
  const related = new Set<string>();
  let cursor = byId.get(focusId);
  while (cursor?.parent && !related.has(cursor.parent)) {
    related.add(cursor.parent);
    cursor = byId.get(cursor.parent);
  }

  const children = new Map<string, string[]>();
  for (const node of nodes) {
    const values = children.get(node.parent) ?? [];
    values.push(node.id);
    children.set(node.parent, values);
  }
  const queue = [...(children.get(focusId) ?? [])];
  for (let index = 0; index < queue.length; index += 1) {
    const id = queue[index];
    if (related.has(id)) {
      continue;
    }
    related.add(id);
    queue.push(...(children.get(id) ?? []));
  }
  return related;
}

function levelIds(
  distances: ReadonlyMap<string, number>,
  depth: number,
  byId: ReadonlyMap<string, ProjectMapNode>,
): string[] {
  return [...distances.entries()]
    .filter(([, distance]) => distance === depth)
    .map(([id]) => id)
    .sort((left, right) => compareNodes(byId, left, right));
}

function levelY(index: number, count: number): number {
  return (index - (count - 1) / 2) * DEPENDENCY_ROW_STEP;
}

export function layoutDependency(
  nodes: readonly ProjectMapNode[],
  dependencies: readonly ProjectMapDependency[],
  focusId: string,
  requestedUpstreamDepth: number,
  requestedDownstreamDepth: number,
): DependencyLayout {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  if (!byId.has(focusId)) {
    throw new Error(`Unknown dependency focus ${focusId}`);
  }

  const upstreamIndex = new Map<string, Neighbor[]>();
  const downstreamIndex = new Map<string, Neighbor[]>();
  for (const dependency of dependencies) {
    const upstream = upstreamIndex.get(dependency.from) ?? [];
    upstream.push({ id: dependency.to });
    upstreamIndex.set(dependency.from, upstream);

    const downstream = downstreamIndex.get(dependency.to) ?? [];
    downstream.push({ id: dependency.from });
    downstreamIndex.set(dependency.to, downstream);
  }

  const upstreamDistances = distancesFrom(
    focusId,
    upstreamIndex,
    byId,
  );
  const downstreamDistances = distancesFrom(
    focusId,
    downstreamIndex,
    byId,
  );
  const maxUpstreamDepth = maximumDistance(upstreamDistances);
  const maxDownstreamDepth = maximumDistance(downstreamDistances);
  const visibleUpstreamDepth = Math.min(
    normalizedDepth(requestedUpstreamDepth),
    maxUpstreamDepth,
  );
  const visibleDownstreamDepth = Math.min(
    normalizedDepth(requestedDownstreamDepth),
    maxDownstreamDepth,
  );

  const positions = new Map<string, DependencyPosition>();
  const causalIds = new Set<string>([focusId]);
  positions.set(focusId, {
    id: focusId,
    role: "focus",
    distance: 0,
    x: 0,
    y: 0,
  });

  for (let depth = 1; depth <= visibleUpstreamDepth; depth += 1) {
    const ids = levelIds(upstreamDistances, depth, byId);
    ids.forEach((id, index) => {
      causalIds.add(id);
      positions.set(id, {
        id,
        role: "upstream",
        distance: depth,
        x: -depth * DEPENDENCY_COLUMN_STEP,
        y: levelY(index, ids.length),
      });
    });
  }
  for (let depth = 1; depth <= visibleDownstreamDepth; depth += 1) {
    const ids = levelIds(downstreamDistances, depth, byId);
    ids.forEach((id, index) => {
      causalIds.add(id);
      positions.set(id, {
        id,
        role: "downstream",
        distance: depth,
        x: depth * DEPENDENCY_COLUMN_STEP,
        y: levelY(index, ids.length),
      });
    });
  }

  const edges = dependencies
    .filter(
      (dependency) =>
        causalIds.has(dependency.from) && causalIds.has(dependency.to),
    )
    .map((dependency) => ({
      // The API stores dependent -> prerequisite. The manuscript reads
      // prerequisite -> dependent.
      from: dependency.to,
      to: dependency.from,
      satisfied: dependency.satisfied,
    }))
    .sort(
      (left, right) =>
        compareNodes(byId, left.from, right.from) ||
        compareNodes(byId, left.to, right.to),
    );

  const hierarchyRelated = hierarchyRelations(focusId, nodes, byId);
  const causalClosure = new Set([
    focusId,
    ...upstreamDistances.keys(),
    ...downstreamDistances.keys(),
  ]);
  const independentSlots = nodes
    .map((node) => node.id)
    .filter(
      (id) => !causalClosure.has(id) && !hierarchyRelated.has(id),
    )
    .sort((left, right) => compareNodes(byId, left, right));
  const parallelCandidates = independentSlots.filter(
    (id) => {
      const node = byId.get(id);
      return node?.readiness === "ready" && node.child_count === 0;
    },
  );

  const causalPositions = [...positions.values()];
  const causalBottom = Math.max(
    MAP_NODE_HEIGHT,
    ...causalPositions.map((position) => position.y + MAP_NODE_HEIGHT),
  );
  const laneColumns = Math.max(
    1,
    visibleUpstreamDepth + visibleDownstreamDepth + 1,
  );
  const laneStartX = -visibleUpstreamDepth * DEPENDENCY_COLUMN_STEP;
  const parallelY = causalBottom + DEPENDENCY_PARALLEL_GAP;
  for (const id of parallelCandidates) {
    const slot = independentSlots.indexOf(id);
    positions.set(id, {
      id,
      role: "parallel",
      distance: 0,
      x:
        laneStartX +
        (slot % laneColumns) * DEPENDENCY_COLUMN_STEP,
      y:
        parallelY +
        Math.floor(slot / laneColumns) * DEPENDENCY_ROW_STEP,
    });
  }

  const allPositions = [...positions.values()];
  const minX = Math.min(...allPositions.map((position) => position.x));
  const minY = Math.min(...allPositions.map((position) => position.y));
  const maxX = Math.max(
    ...allPositions.map((position) => position.x + MAP_NODE_WIDTH),
  );
  const maxY = Math.max(
    ...allPositions.map((position) => position.y + MAP_NODE_HEIGHT),
  );
  const orderedIds = [...allPositions]
    .sort(
      (left, right) =>
        left.y - right.y ||
        left.x - right.x ||
        left.id.localeCompare(right.id),
    )
    .map((position) => position.id);
  const unsatisfiedPrerequisites =
    byId.get(focusId)?.status === "pending"
      ? dependencies
          .filter(
            (dependency) =>
              dependency.from === focusId && !dependency.satisfied,
          )
          .map((dependency) => dependency.to)
          .filter((id) => byId.has(id))
          .sort((left, right) => compareNodes(byId, left, right))
      : [];

  return {
    focusId,
    positions,
    orderedIds,
    causalIds,
    edges,
    parallelCandidates,
    unsatisfiedPrerequisites,
    maxUpstreamDepth,
    maxDownstreamDepth,
    visibleUpstreamDepth,
    visibleDownstreamDepth,
    minX,
    minY,
    maxX,
    maxY,
    width: maxX - minX + DEPENDENCY_SCENE_PADDING * 2,
    height: maxY - minY + DEPENDENCY_SCENE_PADDING * 2,
  };
}
