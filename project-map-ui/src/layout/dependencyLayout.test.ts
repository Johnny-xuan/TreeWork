import { describe, expect, it } from "vitest";
import type {
  ProjectMapDependency,
  ProjectMapNode,
} from "../data/types";
import { layoutDependency } from "./dependencyLayout";

function node(
  id: string,
  options: Partial<ProjectMapNode> = {},
): ProjectMapNode {
  return {
    id,
    parent: "",
    order: 0,
    title: id,
    purpose: `${id} purpose`,
    spec: null,
    status: "pending",
    verification: "unverified",
    status_reason: "",
    is_current: false,
    readiness: "ready",
    depends_on: [],
    child_count: 0,
    ...options,
  };
}

const nodes = [
  node("root", {
    status: "in_progress",
    readiness: "active",
    child_count: 4,
  }),
  node("focus-parent", {
    parent: "root",
    order: 1,
    child_count: 2,
    readiness: "active",
  }),
  node("focus", {
    parent: "focus-parent",
    order: 1,
    readiness: "waiting",
    child_count: 1,
  }),
  node("focus-child", {
    parent: "focus",
    order: 1,
  }),
  node("up-a", { parent: "root", order: 2 }),
  node("up-b", { parent: "root", order: 1 }),
  node("shared-upstream", {
    parent: "root",
    order: 3,
    status: "complete",
    readiness: "complete",
  }),
  node("down-a", { parent: "root", order: 4 }),
  node("down-b", { parent: "root", order: 5 }),
  node("parallel-sibling", {
    parent: "focus-parent",
    order: 3,
  }),
  node("parallel-elsewhere", { parent: "root", order: 6 }),
  node("parallel-container", {
    parent: "root",
    order: 7,
    child_count: 1,
  }),
  node("parallel-container-child", {
    parent: "parallel-container",
    order: 1,
    readiness: "waiting",
  }),
];

const dependencies: ProjectMapDependency[] = [
  { from: "focus", to: "up-a", satisfied: false },
  { from: "focus", to: "up-b", satisfied: true },
  { from: "up-a", to: "shared-upstream", satisfied: true },
  { from: "up-b", to: "shared-upstream", satisfied: true },
  { from: "down-a", to: "focus", satisfied: false },
  { from: "down-b", to: "down-a", satisfied: false },
];

describe("layoutDependency", () => {
  it("follows API dependent-to-prerequisite relations and reverses screen edges", () => {
    const layout = layoutDependency(nodes, dependencies, "focus", 1, 1);
    expect(layout.positions.get("up-a")).toMatchObject({
      role: "upstream",
      distance: 1,
    });
    expect(layout.positions.get("down-a")).toMatchObject({
      role: "downstream",
      distance: 1,
    });
    expect(layout.positions.has("shared-upstream")).toBe(false);
    expect(layout.positions.has("down-b")).toBe(false);
    expect(layout.edges).toEqual(
      expect.arrayContaining([
        { from: "up-a", to: "focus", satisfied: false },
        { from: "up-b", to: "focus", satisfied: true },
        { from: "focus", to: "down-a", satisfied: false },
      ]),
    );
  });

  it("expands each side independently and renders shared nodes once at minimum distance", () => {
    const layout = layoutDependency(nodes, dependencies, "focus", 2, 1);
    expect(layout.positions.get("shared-upstream")).toMatchObject({
      role: "upstream",
      distance: 2,
    });
    expect(
      layout.orderedIds.filter((id) => id === "shared-upstream"),
    ).toHaveLength(1);
    expect(layout.positions.has("down-b")).toBe(false);
    expect(layout.maxUpstreamDepth).toBe(2);
    expect(layout.maxDownstreamDepth).toBe(2);
  });

  it("orders a depth layer by accepted order and then stable branch ID", () => {
    const layout = layoutDependency(nodes, dependencies, "focus", 1, 1);
    expect(layout.positions.get("up-b")!.y).toBeLessThan(
      layout.positions.get("up-a")!.y,
    );
  });

  it("explains waiting with unsatisfied direct prerequisites only", () => {
    const layout = layoutDependency(nodes, dependencies, "focus", 2, 2);
    expect(layout.unsatisfiedPrerequisites).toEqual(["up-a"]);

    const activeFocus = nodes.map((item) =>
      item.id === "focus"
        ? { ...item, status: "in_progress" as const }
        : item,
    );
    expect(
      layoutDependency(
        activeFocus,
        dependencies,
        "focus",
        2,
        2,
      ).unsatisfiedPrerequisites,
    ).toEqual([]);
  });

  it("finds only ready, causally independent, structurally separate candidates", () => {
    const layout = layoutDependency(nodes, dependencies, "focus", 2, 2);
    expect(layout.parallelCandidates).toEqual([
      "parallel-sibling",
      "parallel-elsewhere",
    ]);
    expect(layout.parallelCandidates).not.toContain("root");
    expect(layout.parallelCandidates).not.toContain("focus-parent");
    expect(layout.parallelCandidates).not.toContain("focus-child");
    expect(layout.parallelCandidates).not.toContain("up-a");
    expect(layout.parallelCandidates).not.toContain("down-a");
    expect(layout.parallelCandidates).not.toContain("parallel-container");
  });

  it("does not move existing coordinates for lifecycle, verification, or satisfaction changes", () => {
    const before = layoutDependency(nodes, dependencies, "focus", 2, 2);
    const changedNodes = nodes.map((item) => ({
      ...item,
      status: item.id === "up-a" ? ("complete" as const) : item.status,
      verification: "failed" as const,
      is_current: item.id === "down-a",
      readiness:
        item.id === "parallel-sibling"
          ? ("waiting" as const)
          : item.readiness,
    }));
    const changedDependencies = dependencies.map((dependency) => ({
      ...dependency,
      satisfied: !dependency.satisfied,
    }));
    const after = layoutDependency(
      changedNodes,
      changedDependencies,
      "focus",
      2,
      2,
    );
    for (const [id, position] of before.positions) {
      if (after.positions.has(id)) {
        expect(after.positions.get(id)).toEqual(position);
      }
    }
  });

  it("keeps existing causal coordinates when another level is revealed", () => {
    const direct = layoutDependency(nodes, dependencies, "focus", 1, 1);
    const expanded = layoutDependency(nodes, dependencies, "focus", 2, 2);
    for (const id of ["focus", "up-a", "up-b", "down-a"]) {
      expect(expanded.positions.get(id)).toEqual(
        direct.positions.get(id),
      );
    }
  });
});
