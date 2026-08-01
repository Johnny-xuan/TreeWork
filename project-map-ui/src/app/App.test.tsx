import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { CONNECTION_OFFLINE_AFTER_MS } from "../data/useProjectMapData";
import { App } from "./App";

const nodes = [
  {
    id: "root",
    parent: "",
    order: 0,
    title: "Foundation",
    purpose: "Coordinate the whole project.",
    spec: "spec.md",
    status: "in_progress",
    verification: "partial",
    status_reason: "",
    is_current: false,
    readiness: "active",
    depends_on: [],
    child_count: 6,
  },
  {
    id: "pre-alpha",
    parent: "root",
    order: 1,
    title: "Pre Alpha",
    purpose: "Prepare Alpha.",
    spec: null,
    status: "complete",
    verification: "verified",
    status_reason: "",
    is_current: false,
    readiness: "complete",
    depends_on: [],
    child_count: 0,
  },
  {
    id: "alpha",
    parent: "root",
    order: 2,
    title: "Alpha System",
    purpose: "Build the first system surface.",
    spec: "branches/alpha/spec.md",
    status: "complete",
    verification: "verified",
    status_reason: "",
    is_current: false,
    readiness: "complete",
    depends_on: ["pre-alpha"],
    child_count: 0,
  },
  {
    id: "beta",
    parent: "root",
    order: 3,
    title: "Beta System",
    purpose: "Build the current system surface.",
    spec: "branches/beta/spec.md",
    status: "in_progress",
    verification: "unverified",
    status_reason: "",
    is_current: true,
    readiness: "active",
    depends_on: ["alpha"],
    child_count: 1,
  },
  {
    id: "nested-beta",
    parent: "beta",
    order: 1,
    title: "Nested Beta Work",
    purpose: "Nested scope must not be suggested as parallel.",
    spec: null,
    status: "pending",
    verification: "unverified",
    status_reason: "",
    is_current: false,
    readiness: "ready",
    depends_on: [],
    child_count: 0,
  },
  {
    id: "gamma",
    parent: "root",
    order: 4,
    title: "Gamma Follow-up",
    purpose: "Use Beta after it completes.",
    spec: null,
    status: "pending",
    verification: "unverified",
    status_reason: "",
    is_current: false,
    readiness: "waiting",
    depends_on: ["beta"],
    child_count: 0,
  },
  {
    id: "after-gamma",
    parent: "root",
    order: 5,
    title: "After Gamma",
    purpose: "Use Gamma in another level.",
    spec: null,
    status: "pending",
    verification: "unverified",
    status_reason: "",
    is_current: false,
    readiness: "waiting",
    depends_on: ["gamma"],
    child_count: 0,
  },
  {
    id: "parallel",
    parent: "root",
    order: 6,
    title: "Parallel Candidate",
    purpose: "Proceed without a dependency path.",
    spec: null,
    status: "pending",
    verification: "unverified",
    status_reason: "",
    is_current: false,
    readiness: "ready",
    depends_on: [],
    child_count: 0,
  },
];

const projection = {
  schema_version: 1,
  tree_revision: 2,
  state_event_seq: 7,
  narrative_revision: "sha256:test",
  tree_editing: false,
  projected_at: "unix:1",
  health: { status: "ok", message: "" },
  project: {
    stage: "work_tree",
    current_branch: "beta",
    topology_source: "accepted",
  },
  nodes,
  dependencies: [
    { from: "alpha", to: "pre-alpha", satisfied: true },
    { from: "beta", to: "alpha", satisfied: true },
    { from: "gamma", to: "beta", satisfied: false },
    { from: "after-gamma", to: "gamma", satisfied: false },
  ],
};

const replayTransactions = [
  {
    seq: 1,
    time: "unix:1",
    type: "project.initialized",
    subject: "root",
    message: "Initialized TreeWork",
    tree_revision: 0,
    affected_subjects: ["root"],
    changes: {
      stage: { before: null, after: "alignment" },
      current_branch: { before: null, after: "root" },
    },
    replayable: true,
  },
  {
    seq: 2,
    time: "unix:2",
    type: "tree.applied",
    subject: "root",
    message: "Applied grouped Tree changes",
    tree_revision: 2,
    affected_subjects: ["alpha", "retired-branch"],
    changes: {
      result: { tree_revision: 2, topology_changed: true },
      operations: [
        {
          kind: "create_branch",
          branch: "alpha",
          parent: "root",
          sibling_order: 1,
        },
        {
          kind: "add_dependency",
          branch: "alpha",
          depends_on: "pre-alpha",
        },
      ],
    },
    replayable: true,
  },
  {
    seq: 3,
    time: "unix:3",
    type: "branch.entered",
    subject: "alpha",
    message: "Entered Alpha",
    tree_revision: 2,
    affected_subjects: ["root", "alpha"],
    changes: {
      current_branch: { before: "root", after: "alpha" },
      status: { before: "pending", after: "in_progress" },
      reason: { before: "", after: "" },
    },
    replayable: true,
  },
  {
    seq: 4,
    time: "unix:4",
    type: "verification.recorded",
    subject: "alpha",
    message: "Recorded Alpha verification",
    tree_revision: 2,
    affected_subjects: ["alpha"],
    changes: {
      verification: { before: "unverified", after: "verified" },
      evidence: { command: "npm test", result: "passed", gap: "none" },
    },
    replayable: true,
  },
  {
    seq: 5,
    time: "unix:5",
    type: "branch.entered",
    subject: "beta",
    message: "Entered Beta",
    tree_revision: 2,
    affected_subjects: ["alpha", "beta"],
    changes: {
      current_branch: { before: "alpha", after: "beta" },
      status: { before: "pending", after: "in_progress" },
      reason: { before: "", after: "" },
    },
    replayable: true,
  },
  {
    seq: 6,
    time: "unix:6",
    type: "branch.paused",
    subject: "beta",
    message: "Paused Beta",
    tree_revision: 2,
    affected_subjects: ["beta"],
    changes: {
      status: { before: "in_progress", after: "paused" },
      reason: { before: "", after: "Review" },
    },
    replayable: true,
  },
  {
    seq: 7,
    time: "unix:7",
    type: "branch.entered",
    subject: "beta",
    message: "Returned to Beta",
    tree_revision: 2,
    affected_subjects: ["beta"],
    changes: {
      current_branch: { before: "beta", after: "beta" },
      status: { before: "paused", after: "in_progress" },
      reason: { before: "Review", after: "" },
    },
    replayable: true,
  },
];

let partialReplaySequences = new Set<number>();
let unavailableReplaySequences = new Set<number>();
let includeCheckpointOnlyReplayNode = false;
let replayLiveSeq = 7;
let liveReplayTransactions = [...replayTransactions];

const checkpointOnlyReplayNode = {
  id: "checkpoint-only",
  parent: "root",
  order: 7,
  title: "Checkpoint Only",
  purpose: "Exists in reconstructed history without a matching transaction.",
  spec: null,
  status: "pending",
  verification: "unverified",
  status_reason: "",
  is_current: false,
  readiness: "ready",
  depends_on: [],
  child_count: 0,
};

function replayResponse(url: URL) {
  const at = Number(url.searchParams.get("at") ?? replayLiveSeq);
  const after = Number(url.searchParams.get("after") ?? 0);
  const status = partialReplaySequences.has(at)
    ? "partial"
    : unavailableReplaySequences.has(at)
      ? "unavailable"
      : "available";
  const currentBranch = at < 3 ? "root" : at < 5 ? "alpha" : "beta";
  const replayNodeSource =
    at === 1
      ? nodes.slice(0, 1)
      : includeCheckpointOnlyReplayNode
        ? [...nodes, checkpointOnlyReplayNode]
        : nodes;
  const replayNodes = replayNodeSource.map((node) => ({
    ...node,
    is_current: node.id === currentBranch,
  }));
  return {
    schema_version: 1,
    meta: {
      live_event_seq: replayLiveSeq,
      at_event_seq: at,
      checkpoint_event_seq: at >= 2 ? 2 : 1,
      earliest_replayable_seq: 1,
      tree_revision: at === 1 ? 0 : 2,
      projected_at: `unix:${at}`,
    },
    reconstruction: {
      status,
      gaps:
        status === "available"
          ? []
          : [
              {
                from_seq: at,
                to_seq: at,
                reason: "Synthetic legacy coverage gap",
              },
            ],
    },
    state:
      status === "unavailable"
        ? null
        : {
            tree_editing: false,
            project: {
              ...projection.project,
              current_branch: currentBranch,
              topology_source: at === 1 ? "bootstrap" : "accepted",
            },
            nodes: replayNodes,
            dependencies: projection.dependencies,
          },
    transactions: liveReplayTransactions.filter(
      (transaction) => transaction.seq > after && transaction.seq <= at,
    ),
  };
}

const detail = {
  ...projection,
    branch: nodes.find((node) => node.id === "beta"),
  task_plan: {
    scope: "Build Beta.",
    acceptance: "- [ ] Beta works.",
    local_steps: "1. Implement.",
    out_of_scope: "Replay.",
    dependencies: "Alpha.",
    branch_intake_gate: "Reused the accepted branch.",
  },
  progress: {
    current_reality: "Beta is active.",
    recent_work: "Map geometry is stable.",
    open_issues: "",
    exit_notes: "",
  },
  findings: {
    decisions: "Use SVG.",
    interface_or_contract_effects: "",
    risks_and_unknowns: "",
  },
  verification: {
    status: "unverified",
    evidence: "Unit tests.",
    coverage_gap: "Browser review.",
  },
};

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  onopen: (() => void) | null = null;
  onerror: (() => void) | null = null;
  listeners = new Map<string, (event: MessageEvent<string>) => void>();

  constructor(readonly url: string) {
    FakeEventSource.instances.push(this);
  }

  addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
  ) {
    this.listeners.set(
      type,
      listener as (event: MessageEvent<string>) => void,
    );
  }

  close() {}
}

describe("Project Map application", () => {
  beforeEach(() => {
    window.sessionStorage.clear();
    FakeEventSource.instances = [];
    partialReplaySequences = new Set();
    unavailableReplaySequences = new Set();
    includeCheckpointOnlyReplayNode = false;
    replayLiveSeq = 7;
    liveReplayTransactions = [...replayTransactions];
    vi.stubGlobal("EventSource", FakeEventSource);
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = String(input);
        const parsed = new URL(url, window.location.href);
        if (parsed.pathname === "/api/project-map/replay") {
          return new Response(JSON.stringify(replayResponse(parsed)), {
            status: 200,
            headers: { "content-type": "application/json" },
          });
        }
        const branchId = parsed.searchParams.get(
          "id",
        );
        const body = url.includes("/branch?")
          ? {
              ...detail,
              branch:
                nodes.find((node) => node.id === branchId) ??
                detail.branch,
            }
          : {
              ...projection,
              state_event_seq: replayLiveSeq,
            };
        return new Response(JSON.stringify(body), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }),
    );
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
  });

  it("renders the real Map shell with Dependency and Replay enabled", async () => {
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    expect(screen.getByRole("button", { name: "Map" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(
      screen.getByRole("button", { name: "Dependency" }),
    ).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "Replay" })).not.toBeDisabled();
    expect(document.querySelectorAll(".parent-connectors path")).toHaveLength(
      nodes.length - 1,
    );
  });

  it("keeps App shortcuts unmodified and outside interactive controls", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );

    const dependencyButton = screen.getByRole("button", {
      name: "Dependency",
    });
    dependencyButton.focus();
    fireEvent.keyDown(dependencyButton, { key: "/" });
    expect(document.activeElement).toBe(dependencyButton);

    fireEvent.keyDown(document.body, { key: "/", ctrlKey: true });
    expect(document.activeElement).toBe(dependencyButton);
    fireEvent.keyDown(document.body, { key: "/" });
    expect(document.activeElement).toBe(
      screen.getByLabelText("Search branches"),
    );

    await user.click(dependencyButton);
    await screen.findByTestId("dependency-surface");
    fireEvent.keyDown(document.body, { key: "l", ctrlKey: true });
    expect(screen.queryByLabelText(/Branch details for/)).toBeNull();
    fireEvent.keyDown(document.body, { key: "l" });
    expect(
      await screen.findByLabelText("Branch details for Beta System"),
    ).toBeInTheDocument();
  });

  it("opens a narrative Inspector and keeps annotations in session storage", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(
        document.querySelector<SVGGElement>('[data-node-id="beta"]'),
      ).toBeTruthy(),
    );
    const beta = document.querySelector<SVGGElement>(
      '[data-node-id="beta"]',
    );
    fireEvent.click(beta!);
    expect(
      await screen.findByText("Beta is active."),
    ).toBeInTheDocument();
    const annotation = screen.getByLabelText("Session annotation");
    await user.type(annotation, "Keep this thought");
    await waitFor(() =>
      expect(
        window.sessionStorage.getItem("treework-project-map:v3:/"),
      ).toContain("Keep this thought"),
    );
  });

  it("moves backward and forward through focused branch history", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="alpha"]')).toBeTruthy(),
    );

    const back = screen.getByRole("button", {
      name: "Previous focused branch",
    });
    const forward = screen.getByRole("button", {
      name: "Next focused branch",
    });
    expect(back).toBeDisabled();
    expect(forward).toBeDisabled();

    fireEvent.click(
      document.querySelector<SVGGElement>('[data-node-id="alpha"]')!,
    );
    expect(back).toBeEnabled();
    expect(forward).toBeDisabled();

    await user.click(back);
    expect(
      document.querySelector('[data-node-id="beta"]'),
    ).toHaveAttribute("aria-selected", "true");
    expect(forward).toBeEnabled();

    await user.click(forward);
    expect(
      document.querySelector('[data-node-id="alpha"]'),
    ).toHaveAttribute("aria-selected", "true");
  });

  it("dims search and lifecycle nonmatches without moving node coordinates", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(
        document.querySelector<SVGGElement>('[data-node-id="beta"]'),
      ).toBeTruthy(),
    );
    const beta = document.querySelector<SVGGElement>(
      '[data-node-id="beta"]',
    );
    const before = beta!.getAttribute("transform");
    await user.type(screen.getByLabelText("Search branches"), "Alpha");
    expect(beta).toHaveClass("is-dimmed");
    expect(beta!.getAttribute("transform")).toBe(before);
    await user.selectOptions(
      screen.getByLabelText("Filter by lifecycle status"),
      "complete",
    );
    expect(beta).toHaveClass("is-dimmed");
    expect(beta!.getAttribute("transform")).toBe(before);
  });

  it("switches to a direct focused Dependency view and expands both directions", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    fireEvent.click(
      document.querySelector<SVGGElement>('[data-node-id="beta"]')!,
    );
    await user.click(screen.getByRole("button", { name: "Dependency" }));
    await screen.findByTestId("dependency-surface");
    expect(
      screen.getByRole("navigation", {
        name: "Focused branch hierarchy path",
      }),
    ).toBeInTheDocument();

    expect(
      document.querySelector(
        '[data-node-id="beta"][data-node-role="focus"]',
      ),
    ).toBeTruthy();
    expect(
      document.querySelector(
        '[data-node-id="beta"][role="option"][aria-selected="true"]',
      ),
    ).toBeTruthy();
    expect(
      document.querySelector(
        '[data-node-id="alpha"][data-node-role="upstream"]',
      ),
    ).toBeTruthy();
    expect(
      document.querySelector(
        '[data-node-id="gamma"][data-node-role="downstream"]',
      ),
    ).toBeTruthy();
    expect(
      document.querySelector('[data-node-id="pre-alpha"]'),
    ).toBeNull();
    expect(
      document.querySelector('[data-node-id="after-gamma"]'),
    ).toBeNull();
    expect(
      document.querySelector(
        '[data-node-id="parallel"][data-node-role="parallel"]',
      ),
    ).toBeTruthy();
    expect(
      document.querySelector('[data-node-id="nested-beta"]'),
    ).toBeNull();

    await user.click(
      screen.getByRole("button", { name: "Expand upstream depth" }),
    );
    await user.click(
      screen.getByRole("button", { name: "Expand downstream depth" }),
    );
    expect(
      document.querySelector('[data-node-id="pre-alpha"]'),
    ).toBeTruthy();
    expect(
      document.querySelector('[data-node-id="after-gamma"]'),
    ).toBeTruthy();
  });

  it("keeps Dependency focus when Inspector closes and search replaces it", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    fireEvent.click(
      document.querySelector<SVGGElement>('[data-node-id="beta"]')!,
    );
    await user.click(screen.getByRole("button", { name: "Dependency" }));
    await user.click(
      await screen.findByRole("button", {
        name: "Close branch inspector",
      }),
    );
    expect(screen.queryByLabelText(/Branch details/)).toBeNull();
    expect(
      document.querySelector(
        '[data-node-id="beta"][data-node-role="focus"]',
      ),
    ).toBeTruthy();

    const search = screen.getByLabelText("Search branches");
    await user.clear(search);
    await user.type(search, "After Gamma{Enter}");
    await waitFor(() =>
      expect(
        document.querySelector(
          '[data-node-id="after-gamma"][data-node-role="focus"]',
        ),
      ).toBeTruthy(),
    );
    expect(
      await screen.findByLabelText("Branch details for After Gamma"),
    ).toBeInTheDocument();
  });

  it("returns a Dependency focus to its revealed Map hierarchy context", async () => {
    const user = userEvent.setup();
    window.sessionStorage.setItem(
      "treework-project-map:v3:/",
      JSON.stringify({
        activeView: "dependency",
        selected: "nested-beta",
        inspectorOpen: false,
        collapsed: ["beta"],
        dependencyUpstreamDepth: 2,
        dependencyDownstreamDepth: 2,
      }),
    );
    render(<App />);
    await screen.findByTestId("dependency-surface");
    await user.click(screen.getByRole("button", { name: "Map" }));
    await screen.findByTestId("map-surface");
    const nested = document.querySelector<SVGGElement>(
      '[data-node-id="nested-beta"]',
    );
    expect(nested).toBeTruthy();
    expect(nested).toHaveAttribute("aria-selected", "true");
    expect(
      await screen.findByLabelText("Branch details for Nested Beta Work"),
    ).toBeInTheDocument();
  });

  it("restores Dependency focus and depth preferences from session storage", async () => {
    window.sessionStorage.setItem(
      "treework-project-map:v3:/",
      JSON.stringify({
        activeView: "dependency",
        selected: "gamma",
        inspectorOpen: false,
        dependencyUpstreamDepth: 2,
        dependencyDownstreamDepth: 3,
      }),
    );
    render(<App />);
    await screen.findByTestId("dependency-surface");
    expect(
      document.querySelector(
        '[data-node-id="gamma"][data-node-role="focus"]',
      ),
    ).toBeTruthy();
    expect(screen.getByLabelText("Visible upstream depth")).toHaveTextContent(
      "2",
    );
    expect(
      JSON.parse(
        window.sessionStorage.getItem("treework-project-map:v3:/")!,
      ).dependencyDownstreamDepth,
    ).toBe(3);
  });

  it("dims Dependency presentation without moving causal coordinates", async () => {
    const user = userEvent.setup();
    window.sessionStorage.setItem(
      "treework-project-map:v3:/",
      JSON.stringify({
        activeView: "dependency",
        selected: "beta",
        inspectorOpen: false,
      }),
    );
    render(<App />);
    await screen.findByTestId("dependency-surface");
    const causal = [
      ...document.querySelectorAll<SVGGElement>(
        '[data-node-role="upstream"], [data-node-role="focus"], [data-node-role="downstream"]',
      ),
    ];
    const before = causal.map((item) => item.getAttribute("transform"));
    await user.type(screen.getByLabelText("Search branches"), "Alpha");
    await user.selectOptions(
      screen.getByLabelText("Filter by lifecycle status"),
      "complete",
    );
    expect(causal.map((item) => item.getAttribute("transform"))).toEqual(
      before,
    );
  });

  it("plays, pauses, steps, scrubs, changes speed, and uses Replay keyboard controls", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    await screen.findByLabelText("Replay timeline");
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "7",
      ),
    );
    expect(
      screen.queryByLabelText(/Branch details for/),
    ).not.toBeInTheDocument();
    expect(screen.getByText("Live", { exact: true })).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "Previous transaction" }),
    );
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "6",
      ),
    );
    fireEvent.keyDown(window, { key: "ArrowLeft" });
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "5",
      ),
    );

    fireEvent.change(screen.getByLabelText("Replay transaction"), {
      target: { value: "1" },
    });
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "2",
      ),
    );
    expect(
      screen.queryByText("Moved alpha from root to delivery.", {
        exact: true,
      }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByText("Created Alpha System · alpha under Foundation · root at sibling position 2."),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Alpha System · alpha now depends on Pre Alpha · pre-alpha."),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "4 times speed" }));
    expect(
      screen.getByRole("button", { name: "4 times speed" }),
    ).toHaveAttribute("aria-pressed", "true");
    await user.click(screen.getByRole("button", { name: "Play Replay" }));
    await waitFor(
      () =>
        expect(document.querySelector(".replay-view")).toHaveAttribute(
          "data-replay-seq",
          "3",
        ),
      { timeout: 1200 },
    );
    await user.click(screen.getByRole("button", { name: "Pause Replay" }));
  });

  it("keeps Replay shortcuts out of branch tree and option keyboard handling", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "7",
      ),
    );

    const replayView = document.querySelector(".replay-view")!;
    const stage = screen.getByTestId("replay-stage");
    const modifiedArrow = new KeyboardEvent("keydown", {
      key: "ArrowLeft",
      altKey: true,
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(modifiedArrow);
    expect(modifiedArrow.defaultPrevented).toBe(false);
    expect(replayView).toHaveAttribute("data-replay-seq", "7");

    const prevented = new KeyboardEvent("keydown", {
      key: "ArrowLeft",
      bubbles: true,
      cancelable: true,
    });
    prevented.preventDefault();
    stage.dispatchEvent(prevented);
    expect(replayView).toHaveAttribute("data-replay-seq", "7");

    const beta = document.querySelector<SVGGElement>(
      '.replay-stage .branch-node[data-node-id="beta"][role="treeitem"]',
    )!;
    beta.focus();
    fireEvent.keyDown(beta, { key: "ArrowLeft" });
    expect(document.activeElement).toBe(
      document.querySelector(
        '.replay-stage .branch-node[data-node-id="root"]',
      ),
    );
    expect(replayView).toHaveAttribute("data-replay-seq", "7");

    fireEvent.keyDown(beta, { key: "ArrowRight" });
    expect(document.activeElement).toBe(
      document.querySelector(
        '.replay-stage .branch-node[data-node-id="nested-beta"]',
      ),
    );
    expect(replayView).toHaveAttribute("data-replay-seq", "7");

    fireEvent.keyDown(beta, { key: " " });
    expect(screen.getByLabelText("Filter Replay by branch")).toHaveValue("beta");
    expect(screen.getByRole("button", { name: "Play Replay" })).toBeVisible();
    expect(replayView).toHaveAttribute("data-replay-seq", "7");

    const option = document.createElement("div");
    option.setAttribute("role", "option");
    option.tabIndex = 0;
    replayView.append(option);
    option.focus();
    fireEvent.keyDown(option, { key: "ArrowLeft" });
    fireEvent.keyDown(option, { key: " " });
    expect(replayView).toHaveAttribute("data-replay-seq", "7");
    expect(screen.getByRole("button", { name: "Play Replay" })).toBeVisible();
    option.remove();
  });

  it("removes an active Replay transition when reduced motion becomes enabled", async () => {
    let reducedMotion = false;
    const listeners = new Set<() => void>();
    vi.stubGlobal("matchMedia", (query: string) => ({
      get matches() {
        return reducedMotion;
      },
      media: query,
      onchange: null,
      addEventListener: (_type: string, listener: () => void) =>
        listeners.add(listener),
      removeEventListener: (_type: string, listener: () => void) =>
        listeners.delete(listener),
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => true,
    }));

    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    const previous = await screen.findByRole("button", {
      name: "Previous transaction",
    });
    await user.click(previous);
    await user.click(previous);
    await user.click(previous);
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "4",
      ),
    );
    expect(
      document.querySelector(
        '.branch-node-visual[class*="is-replay-"]',
      ),
    ).toBeTruthy();

    act(() => {
      reducedMotion = true;
      listeners.forEach((listener) => listener());
    });
    expect(
      document.querySelector(
        '.branch-node-visual[class*="is-replay-"]',
      ),
    ).toBeNull();
    expect(document.querySelector(".replay-exiting-nodes")).toBeNull();
  });

  it("finishes an active Replay transition after playback speed changes", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    const previous = await screen.findByRole("button", {
      name: "Previous transaction",
    });
    await user.click(previous);
    await user.click(previous);
    await user.click(previous);
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "4",
      ),
    );
    expect(
      document.querySelector(
        '.branch-node-visual[class*="is-replay-"]',
      ),
    ).toBeTruthy();

    await user.click(screen.getByRole("button", { name: "4 times speed" }));
    await waitFor(
      () =>
        expect(
          document.querySelector(
            '.branch-node-visual[class*="is-replay-"]',
          ),
        ).toBeNull(),
      { timeout: 700 },
    );
    expect(document.querySelector(".replay-exiting-nodes")).toBeNull();
  });

  it("keeps shared Canvas settings available beside Replay controls", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    await screen.findByLabelText("Replay timeline");

    expect(
      screen.getByRole("button", {
        name: "Refresh Replay and accepted state",
      }),
    ).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: "Open canvas settings" }),
    );
    expect(screen.getByLabelText("Canvas settings")).toBeVisible();
    expect(screen.getByLabelText("Replay timeline")).toBeVisible();
    expect(
      document.querySelector(".replay-transaction-detail"),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText(/Branch details for/)).not.toBeInTheDocument();

    fireEvent.change(document.querySelector("#panSensitivity")!, {
      target: { value: "1.25" },
    });
    fireEvent.change(document.querySelector("#zoomSensitivity")!, {
      target: { value: "0.75" },
    });
    expect(document.querySelector("#panSensitivityValue")).toHaveTextContent(
      "125%",
    );
    expect(document.querySelector("#zoomSensitivityValue")).toHaveTextContent(
      "75%",
    );
  });

  it("filters by stable historical branch ID without filtering the reconstructed Tree", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    const filter = await screen.findByLabelText("Filter Replay by branch");
    await user.selectOptions(filter, "retired-branch");
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "2",
      ),
    );
    expect(filter).toHaveValue("retired-branch");
    expect(
      screen.getByRole("option", { name: "retired-branch" }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(document.querySelectorAll(".map-content .branch-node")).toHaveLength(
        nodes.length,
      ),
    );

    fireEvent.keyDown(filter, { key: "ArrowLeft" });
    expect(document.querySelector(".replay-view")).toHaveAttribute(
      "data-replay-seq",
      "2",
    );
    await user.click(screen.getByRole("button", { name: "Return to Live" }));
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "7",
      ),
    );
    expect(filter).toHaveValue("");
    expect(document.querySelector(".replay-view")).toHaveAttribute(
      "data-replay-live",
      "true",
    );
  });

  it("keeps a reconstructed checkpoint-only branch usable with an empty timeline", async () => {
    includeCheckpointOnlyReplayNode = true;
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    await user.click(
      await screen.findByRole("button", { name: "Previous transaction" }),
    );
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "6",
      ),
    );
    const checkpointNode = await waitFor(() => {
      const node = document.querySelector<SVGGElement>(
        '.replay-stage [data-node-id="checkpoint-only"]',
      );
      expect(node).toBeTruthy();
      return node!;
    });
    fireEvent.click(checkpointNode);

    expect(screen.getByLabelText("Filter Replay by branch")).toHaveValue(
      "checkpoint-only",
    );
    expect(
      screen.getByText("No accepted transactions mention this branch."),
    ).toBeInTheDocument();
    expect(document.querySelector(".replay-view")).toHaveAttribute(
      "data-replay-seq",
      "6",
    );
    expect(screen.getByText("Seq 6", { exact: true })).toBeInTheDocument();
    expect(document.querySelector(".replay-stage #projectMapSvg")).toBeTruthy();
    expect(
      screen.queryByText("Reconstructing sequence", { exact: false }),
    ).not.toBeInTheDocument();
  });

  it("withholds every scene for partial and unavailable reconstruction", async () => {
    partialReplaySequences.add(6);
    unavailableReplaySequences.add(5);
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    await screen.findByText("Returned to Beta", { exact: true });
    await user.click(
      screen.getByRole("button", { name: "Previous transaction" }),
    );
    expect(
      await screen.findByText("Historical coverage is partial"),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Synthetic legacy coverage gap", { exact: false }),
    ).toBeInTheDocument();
    expect(document.querySelector("#projectMapSvg")).toBeNull();
    expect(screen.getByText("Paused Beta", { exact: true })).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "Previous transaction" }),
    );
    expect(
      await screen.findByText("Historical scene is unavailable"),
    ).toBeInTheDocument();
    expect(document.querySelector("#projectMapSvg")).toBeNull();
    expect(screen.getByText("Entered Beta", { exact: true })).toBeInTheDocument();
  });

  it("refreshes a live catalog without moving a historical cursor", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    await user.click(
      await screen.findByRole("button", { name: "Previous transaction" }),
    );
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "6",
      ),
    );

    replayLiveSeq = 8;
    liveReplayTransactions = [
      ...liveReplayTransactions,
      {
        ...replayTransactions[6],
        seq: 8,
        time: "unix:8",
        message: "New accepted live transaction",
      },
    ];
    FakeEventSource.instances[0].listeners.get("invalidate")?.(
      new MessageEvent("invalidate", {
        data: JSON.stringify({
          schema_version: 1,
          kind: "project_map.invalidated",
          changes: ["events", "state"],
          tree_revision: 2,
          state_event_seq: 8,
          narrative_revision: "sha256:live-8",
        }),
      }),
    );
    await screen.findByText("Live seq 8", { exact: true });
    expect(document.querySelector(".replay-view")).toHaveAttribute(
      "data-replay-seq",
      "6",
    );

    await user.click(screen.getByRole("button", { name: "Return to Live" }));
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "8",
      ),
    );
    replayLiveSeq = 9;
    liveReplayTransactions = [
      ...liveReplayTransactions,
      {
        ...replayTransactions[6],
        seq: 9,
        time: "unix:9",
        message: "Followed accepted live transaction",
      },
    ];
    FakeEventSource.instances[0].listeners.get("invalidate")?.(
      new MessageEvent("invalidate", {
        data: JSON.stringify({
          schema_version: 1,
          kind: "project_map.invalidated",
          changes: ["events", "state"],
          tree_revision: 2,
          state_event_seq: 9,
          narrative_revision: "sha256:live-9",
        }),
      }),
    );
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "9",
      ),
    );
  });

  it("converges after a missed invalidation without moving Replay history", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    act(() => FakeEventSource.instances[0].onopen?.());
    await user.click(screen.getByRole("button", { name: "Replay" }));
    await user.click(
      await screen.findByRole("button", { name: "Previous transaction" }),
    );
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "6",
      ),
    );

    replayLiveSeq = 8;
    liveReplayTransactions = [
      ...liveReplayTransactions,
      {
        ...replayTransactions[6],
        seq: 8,
        time: "unix:8",
        message: "Missed while disconnected",
      },
    ];
    act(() => FakeEventSource.instances[0].onerror?.());
    expect(
      await screen.findByText("Live updates are reconnecting."),
    ).toBeInTheDocument();
    act(() => FakeEventSource.instances[0].onopen?.());
    await screen.findByText("Live seq 8", { exact: true });
    expect(document.querySelector(".replay-view")).toHaveAttribute(
      "data-replay-seq",
      "6",
    );
  });

  it("authoritatively refetches when the first event stream opens", async () => {
    const user = userEvent.setup();
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    const fetchMock = vi.mocked(fetch);
    const callsBeforeOpen = fetchMock.mock.calls.length;
    replayLiveSeq = 8;
    liveReplayTransactions = [
      ...liveReplayTransactions,
      {
        ...replayTransactions[6],
        seq: 8,
        time: "unix:8",
        message: "Changed between initial GET and first subscribe",
      },
    ];

    act(() => FakeEventSource.instances[0].onopen?.());

    await waitFor(() =>
      expect(fetchMock.mock.calls.length).toBeGreaterThan(callsBeforeOpen),
    );
    await user.click(screen.getByRole("button", { name: "Replay" }));
    await waitFor(() =>
      expect(document.querySelector(".replay-view")).toHaveAttribute(
        "data-replay-seq",
        "8",
      ),
    );
  });

  it("latches offline after sustained errors until the stream opens", async () => {
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    vi.useFakeTimers();
    try {
      act(() => {
        FakeEventSource.instances[0].onerror?.();
        vi.advanceTimersByTime(4000);
        FakeEventSource.instances[0].onerror?.();
        vi.advanceTimersByTime(1001);
      });
      expect(
        screen.getByText(
          "Live updates are unavailable. Manual refresh remains available.",
        ),
      ).toBeInTheDocument();
      act(() => {
        FakeEventSource.instances[0].onerror?.();
        FakeEventSource.instances[0].onerror?.();
        vi.advanceTimersByTime(CONNECTION_OFFLINE_AFTER_MS * 2);
      });
      expect(
        screen.getByText(
          "Live updates are unavailable. Manual refresh remains available.",
        ),
      ).toBeInTheDocument();
      expect(
        screen.queryByText("Live updates are reconnecting."),
      ).not.toBeInTheDocument();
      act(() => FakeEventSource.instances[0].onopen?.());
      expect(
        screen.queryByText(
          "Live updates are unavailable. Manual refresh remains available.",
        ),
      ).not.toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("shows explicit offline state and refetches when the browser returns online", async () => {
    render(<App />);
    await waitFor(() =>
      expect(document.querySelector('[data-node-id="beta"]')).toBeTruthy(),
    );
    const fetchMock = vi.mocked(fetch);
    const callsBefore = fetchMock.mock.calls.length;
    fireEvent(window, new Event("offline"));
    expect(
      await screen.findByText(
        "Live updates are unavailable. Manual refresh remains available.",
      ),
    ).toBeInTheDocument();
    fireEvent(window, new Event("online"));
    await waitFor(() =>
      expect(fetchMock.mock.calls.length).toBeGreaterThan(callsBefore),
    );
    expect(
      screen.getByText(
        "Live updates are unavailable. Manual refresh remains available.",
      ),
    ).toBeInTheDocument();
    act(() => FakeEventSource.instances[0].onopen?.());
    expect(
      screen.queryByText(
        "Live updates are unavailable. Manual refresh remains available.",
      ),
    ).not.toBeInTheDocument();
  });
});
