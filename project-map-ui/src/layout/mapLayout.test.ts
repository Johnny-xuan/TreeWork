import { describe, expect, it } from "vitest";
import type { ProjectMapNode } from "../data/types";
import {
  currentRoute,
  layoutMap,
  MAP_COLUMN_STEP,
  subtreeStatusCounts,
} from "./mapLayout";

function node(
  id: string,
  parent: string,
  order: number,
  status: ProjectMapNode["status"] = "pending",
): ProjectMapNode {
  return {
    id,
    parent,
    order,
    title: id,
    purpose: `${id} purpose`,
    spec: null,
    status,
    verification: "unverified",
    status_reason: "",
    is_current: false,
    readiness: status === "complete" ? "complete" : "ready",
    depends_on: [],
    child_count: 0,
  };
}

const fixture = [
  node("root", "", 0, "in_progress"),
  node("second", "root", 2),
  node("first", "root", 1),
  node("first-b", "first", 2),
  node("first-a", "first", 1, "complete"),
  node("deep", "first-a", 1),
];

describe("layoutMap", () => {
  it("keeps accepted sibling order and contiguous subtrees", () => {
    const layout = layoutMap(fixture, new Set());
    const order = layout.orderedIds;
    expect(order.indexOf("first")).toBeLessThan(order.indexOf("second"));
    expect(order.indexOf("first-a")).toBeLessThan(order.indexOf("first-b"));
    expect(order.indexOf("deep")).toBeLessThan(order.indexOf("first-b"));
  });

  it("aligns every parent-child depth to a stable column", () => {
    const layout = layoutMap(fixture, new Set());
    const root = layout.positions.get("root")!;
    const first = layout.positions.get("first")!;
    const second = layout.positions.get("second")!;
    const deep = layout.positions.get("deep")!;
    expect(first.x - root.x).toBe(MAP_COLUMN_STEP);
    expect(second.x).toBe(first.x);
    expect(deep.x - root.x).toBe(MAP_COLUMN_STEP * 3);
  });

  it("does not move coordinates when lifecycle or verification changes", () => {
    const before = layoutMap(fixture, new Set());
    const changed = fixture.map((item) => ({
      ...item,
      status: "aborted" as const,
      verification: "failed" as const,
      readiness: "aborted" as const,
    }));
    const after = layoutMap(changed, new Set());
    expect([...after.positions.values()]).toEqual([
      ...before.positions.values(),
    ]);
  });

  it("collapses descendants and reports their stable count", () => {
    const layout = layoutMap(fixture, new Set(["first"]));
    expect(layout.positions.has("first-a")).toBe(false);
    expect(layout.positions.get("first")?.hiddenDescendants).toBe(3);
  });
});

describe("map derivations", () => {
  it("finds the complete root-to-current route", () => {
    expect([...currentRoute(fixture, "deep")]).toEqual([
      "deep",
      "first-a",
      "first",
      "root",
    ]);
  });

  it("summarizes descendant lifecycle without changing geometry", () => {
    const counts = subtreeStatusCounts(fixture);
    expect(counts.get("first")).toEqual({ complete: 1, pending: 2 });
  });
});
