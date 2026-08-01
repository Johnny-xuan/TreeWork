import { hierarchy, tree } from "d3";
import type { ProjectMapNode } from "../data/types";

export const MAP_NODE_WIDTH = 224;
export const MAP_NODE_HEIGHT = 82;
export const MAP_COLUMN_STEP = 360;
export const MAP_ROW_STEP = 112;
export const MAP_PADDING_X = 72;
export const MAP_PADDING_Y = 64;

interface TreeDatum {
  node: ProjectMapNode;
  children: TreeDatum[];
}

export interface MapPosition {
  id: string;
  parent: string;
  depth: number;
  x: number;
  y: number;
  hiddenDescendants: number;
}

export interface MapLink {
  from: string;
  to: string;
}

export interface MapLayout {
  positions: Map<string, MapPosition>;
  orderedIds: string[];
  links: MapLink[];
  depthCount: number;
  width: number;
  height: number;
}

function buildTree(nodes: readonly ProjectMapNode[]): {
  root: TreeDatum;
  byId: Map<string, TreeDatum>;
} {
  const byId = new Map<string, TreeDatum>();
  for (const node of nodes) {
    if (byId.has(node.id)) {
      throw new Error(`Duplicate branch ${node.id}`);
    }
    byId.set(node.id, { node, children: [] });
  }
  const roots: TreeDatum[] = [];
  for (const item of byId.values()) {
    if (!item.node.parent) {
      roots.push(item);
      continue;
    }
    const parent = byId.get(item.node.parent);
    if (!parent) {
      throw new Error(
        `Branch ${item.node.id} references missing parent ${item.node.parent}`,
      );
    }
    parent.children.push(item);
  }
  if (roots.length !== 1) {
    throw new Error(`Expected one root branch, found ${roots.length}`);
  }
  for (const item of byId.values()) {
    item.children.sort(
      (left, right) =>
        left.node.order - right.node.order ||
        left.node.id.localeCompare(right.node.id),
    );
  }
  return { root: roots[0], byId };
}

function descendantCount(item: TreeDatum): number {
  return item.children.reduce(
    (total, child) => total + 1 + descendantCount(child),
    0,
  );
}

export function layoutMap(
  nodes: readonly ProjectMapNode[],
  collapsed: ReadonlySet<string>,
): MapLayout {
  if (nodes.length === 0) {
    return {
      positions: new Map(),
      orderedIds: [],
      links: [],
      depthCount: 0,
      width: 0,
      height: 0,
    };
  }
  const { root, byId } = buildTree(nodes);
  const hierarchyRoot = hierarchy(
    root,
    (item) => (collapsed.has(item.node.id) ? null : item.children),
  );
  const laidOut = tree<TreeDatum>().nodeSize([
    MAP_ROW_STEP,
    MAP_COLUMN_STEP,
  ])(hierarchyRoot);
  const descendants = laidOut.descendants();
  const minimumVertical = Math.min(...descendants.map((item) => item.x));
  const positions = new Map<string, MapPosition>();
  let maximumDepth = 0;
  let maximumVertical = 0;

  for (const item of descendants) {
    const id = item.data.node.id;
    const x = MAP_PADDING_X + item.y;
    const y = MAP_PADDING_Y + item.x - minimumVertical;
    maximumDepth = Math.max(maximumDepth, item.depth);
    maximumVertical = Math.max(maximumVertical, y);
    positions.set(id, {
      id,
      parent: item.data.node.parent,
      depth: item.depth,
      x,
      y,
      hiddenDescendants: collapsed.has(id)
        ? descendantCount(byId.get(id)!)
        : 0,
    });
  }

  const links = descendants
    .filter((item) => item.parent)
    .map((item) => ({
      from: item.parent!.data.node.id,
      to: item.data.node.id,
    }));
  const orderedIds = [...positions.values()]
    .sort((left, right) => left.y - right.y || left.x - right.x)
    .map((position) => position.id);

  return {
    positions,
    orderedIds,
    links,
    depthCount: maximumDepth + 1,
    width:
      MAP_PADDING_X * 2 +
      maximumDepth * MAP_COLUMN_STEP +
      MAP_NODE_WIDTH,
    height: maximumVertical + MAP_NODE_HEIGHT + MAP_PADDING_Y,
  };
}

export function currentRoute(
  nodes: readonly ProjectMapNode[],
  currentBranch: string,
): Set<string> {
  const byId = new Map(nodes.map((node) => [node.id, node]));
  const route = new Set<string>();
  let cursor = byId.get(currentBranch);
  while (cursor && !route.has(cursor.id)) {
    route.add(cursor.id);
    cursor = cursor.parent ? byId.get(cursor.parent) : undefined;
  }
  return route;
}

export function subtreeStatusCounts(
  nodes: readonly ProjectMapNode[],
): Map<string, Record<string, number>> {
  const children = new Map<string, ProjectMapNode[]>();
  for (const node of nodes) {
    const siblings = children.get(node.parent) ?? [];
    siblings.push(node);
    children.set(node.parent, siblings);
  }
  const result = new Map<string, Record<string, number>>();
  const collect = (id: string): Record<string, number> => {
    const counts: Record<string, number> = {};
    for (const child of children.get(id) ?? []) {
      counts[child.status] = (counts[child.status] ?? 0) + 1;
      for (const [status, count] of Object.entries(collect(child.id))) {
        counts[status] = (counts[status] ?? 0) + count;
      }
    }
    result.set(id, counts);
    return counts;
  };
  const root = nodes.find((node) => !node.parent);
  if (root) {
    collect(root.id);
  }
  return result;
}
