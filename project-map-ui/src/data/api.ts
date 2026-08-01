import type {
  ApiFailure,
  BranchDetail,
  ProjectMapInvalidation,
  ProjectMapDependency,
  ProjectMapNode,
  ProjectMapProjection,
  ReplayResponse,
  ReplayState,
  ReplayTransaction,
} from "./types";

const lifecycleStatuses = new Set([
  "pending",
  "in_progress",
  "paused",
  "complete",
  "aborted",
]);
const verificationStatuses = new Set([
  "unverified",
  "partial",
  "verified",
  "failed",
]);
const readinessStatuses = new Set([
  "active",
  "ready",
  "waiting",
  "paused",
  "complete",
  "aborted",
]);
const invalidationCategories = new Set([
  "topology",
  "state",
  "narrative",
  "events",
  "health",
]);
const replayReconstructionStatuses = new Set([
  "available",
  "partial",
  "unavailable",
]);

export class ProjectMapApiError extends Error {
  readonly status: number;
  readonly healthStatus: string | null;

  constructor(message: string, status: number, healthStatus: string | null) {
    super(message);
    this.name = "ProjectMapApiError";
    this.status = status;
    this.healthStatus = healthStatus;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function stringField(
  value: Record<string, unknown>,
  field: string,
  context: string,
): string {
  const candidate = value[field];
  if (typeof candidate !== "string") {
    throw new ProjectMapApiError(
      `${context}.${field} is not a string`,
      502,
      null,
    );
  }
  return candidate;
}

function numberField(
  value: Record<string, unknown>,
  field: string,
  context: string,
): number {
  const candidate = value[field];
  if (
    typeof candidate !== "number" ||
    !Number.isSafeInteger(candidate) ||
    candidate < 0
  ) {
    throw new ProjectMapApiError(
      `${context}.${field} is not a non-negative integer`,
      502,
      null,
    );
  }
  return candidate;
}

function nullableNumberField(
  value: Record<string, unknown>,
  field: string,
  context: string,
): number | null {
  if (value[field] === null) {
    return null;
  }
  return numberField(value, field, context);
}

function booleanField(
  value: Record<string, unknown>,
  field: string,
  context: string,
): boolean {
  const candidate = value[field];
  if (typeof candidate !== "boolean") {
    throw new ProjectMapApiError(
      `${context}.${field} is not a boolean`,
      502,
      null,
    );
  }
  return candidate;
}

function stringArray(
  value: Record<string, unknown>,
  field: string,
  context: string,
): string[] {
  const candidate = value[field];
  if (
    !Array.isArray(candidate) ||
    candidate.some((item) => typeof item !== "string")
  ) {
    throw new ProjectMapApiError(
      `${context}.${field} is not a string array`,
      502,
      null,
    );
  }
  return candidate;
}

function parseHealth(value: unknown) {
  if (!isRecord(value)) {
    throw new ProjectMapApiError("health is not an object", 502, null);
  }
  return {
    status: stringField(value, "status", "health"),
    message: stringField(value, "message", "health"),
  };
}

function parseNode(value: unknown, index: number): ProjectMapNode {
  if (!isRecord(value)) {
    throw new ProjectMapApiError(`nodes[${index}] is not an object`, 502, null);
  }
  const status = stringField(value, "status", `nodes[${index}]`);
  const verification = stringField(
    value,
    "verification",
    `nodes[${index}]`,
  );
  const readiness = stringField(value, "readiness", `nodes[${index}]`);
  if (!lifecycleStatuses.has(status)) {
    throw new ProjectMapApiError(
      `nodes[${index}].status is unsupported`,
      502,
      null,
    );
  }
  if (!verificationStatuses.has(verification)) {
    throw new ProjectMapApiError(
      `nodes[${index}].verification is unsupported`,
      502,
      null,
    );
  }
  if (!readinessStatuses.has(readiness)) {
    throw new ProjectMapApiError(
      `nodes[${index}].readiness is unsupported`,
      502,
      null,
    );
  }
  const spec = value.spec;
  if (spec !== null && typeof spec !== "string") {
    throw new ProjectMapApiError(
      `nodes[${index}].spec is invalid`,
      502,
      null,
    );
  }
  return {
    id: stringField(value, "id", `nodes[${index}]`),
    parent: stringField(value, "parent", `nodes[${index}]`),
    order: numberField(value, "order", `nodes[${index}]`),
    title: stringField(value, "title", `nodes[${index}]`),
    purpose: stringField(value, "purpose", `nodes[${index}]`),
    spec,
    status: status as ProjectMapNode["status"],
    verification: verification as ProjectMapNode["verification"],
    status_reason: stringField(value, "status_reason", `nodes[${index}]`),
    is_current: booleanField(value, "is_current", `nodes[${index}]`),
    readiness: readiness as ProjectMapNode["readiness"],
    depends_on: stringArray(value, "depends_on", `nodes[${index}]`),
    child_count: numberField(value, "child_count", `nodes[${index}]`),
  };
}

function parseDependencies(value: unknown): ProjectMapDependency[] {
  if (!Array.isArray(value)) {
    throw new ProjectMapApiError(
      "dependencies is not an array",
      502,
      null,
    );
  }
  return value.map((dependency, index) => {
    if (!isRecord(dependency)) {
      throw new ProjectMapApiError(
        `dependencies[${index}] is not an object`,
        502,
        null,
      );
    }
    return {
      from: stringField(dependency, "from", `dependencies[${index}]`),
      to: stringField(dependency, "to", `dependencies[${index}]`),
      satisfied: booleanField(
        dependency,
        "satisfied",
        `dependencies[${index}]`,
      ),
    };
  });
}

export function parseProjection(value: unknown): ProjectMapProjection {
  if (!isRecord(value)) {
    throw new ProjectMapApiError("projection is not an object", 502, null);
  }
  const project = value.project;
  const dependencies = value.dependencies;
  const nodes = value.nodes;
  if (!isRecord(project)) {
    throw new ProjectMapApiError("project is not an object", 502, null);
  }
  if (!Array.isArray(nodes) || !Array.isArray(dependencies)) {
    throw new ProjectMapApiError(
      "projection nodes or dependencies are invalid",
      502,
      null,
    );
  }
  return {
    schema_version: numberField(value, "schema_version", "projection"),
    tree_revision: numberField(value, "tree_revision", "projection"),
    state_event_seq: numberField(value, "state_event_seq", "projection"),
    narrative_revision: stringField(
      value,
      "narrative_revision",
      "projection",
    ),
    tree_editing: booleanField(value, "tree_editing", "projection"),
    projected_at: stringField(value, "projected_at", "projection"),
    health: parseHealth(value.health),
    project: {
      stage: stringField(project, "stage", "project"),
      current_branch: stringField(project, "current_branch", "project"),
      topology_source: stringField(project, "topology_source", "project"),
    },
    nodes: nodes.map(parseNode),
    dependencies: parseDependencies(dependencies),
  };
}

function parseReplayState(value: unknown): ReplayState {
  if (!isRecord(value)) {
    throw new ProjectMapApiError(
      "replay state is not an object",
      502,
      null,
    );
  }
  const project = value.project;
  const nodes = value.nodes;
  if (!isRecord(project) || !Array.isArray(nodes)) {
    throw new ProjectMapApiError(
      "replay state project or nodes are invalid",
      502,
      null,
    );
  }
  return {
    tree_editing: booleanField(value, "tree_editing", "replay.state"),
    project: {
      stage: stringField(project, "stage", "replay.state.project"),
      current_branch: stringField(
        project,
        "current_branch",
        "replay.state.project",
      ),
      topology_source: stringField(
        project,
        "topology_source",
        "replay.state.project",
      ),
    },
    nodes: nodes.map(parseNode),
    dependencies: parseDependencies(value.dependencies),
  };
}

function parseReplayTransaction(
  value: unknown,
  index: number,
): ReplayTransaction {
  if (!isRecord(value)) {
    throw new ProjectMapApiError(
      `transactions[${index}] is not an object`,
      502,
      null,
    );
  }
  if (!Object.hasOwn(value, "changes")) {
    throw new ProjectMapApiError(
      `transactions[${index}].changes is missing`,
      502,
      null,
    );
  }
  const replayabilityReason = value.replayability_reason;
  if (
    replayabilityReason !== undefined &&
    replayabilityReason !== null &&
    typeof replayabilityReason !== "string"
  ) {
    throw new ProjectMapApiError(
      `transactions[${index}].replayability_reason is invalid`,
      502,
      null,
    );
  }
  return {
    seq: numberField(value, "seq", `transactions[${index}]`),
    time: stringField(value, "time", `transactions[${index}]`),
    type: stringField(value, "type", `transactions[${index}]`),
    subject: stringField(value, "subject", `transactions[${index}]`),
    message: stringField(value, "message", `transactions[${index}]`),
    tree_revision: nullableNumberField(
      value,
      "tree_revision",
      `transactions[${index}]`,
    ),
    affected_subjects: stringArray(
      value,
      "affected_subjects",
      `transactions[${index}]`,
    ),
    changes: value.changes,
    replayable: booleanField(
      value,
      "replayable",
      `transactions[${index}]`,
    ),
    replayability_reason: replayabilityReason ?? null,
  };
}

export function parseReplayResponse(value: unknown): ReplayResponse {
  if (!isRecord(value)) {
    throw new ProjectMapApiError(
      "replay response is not an object",
      502,
      null,
    );
  }
  const meta = value.meta;
  const reconstruction = value.reconstruction;
  const transactions = value.transactions;
  if (
    !isRecord(meta) ||
    !isRecord(reconstruction) ||
    !Array.isArray(transactions)
  ) {
    throw new ProjectMapApiError(
      "replay metadata, reconstruction, or transactions are invalid",
      502,
      null,
    );
  }
  const status = stringField(
    reconstruction,
    "status",
    "replay.reconstruction",
  );
  if (!replayReconstructionStatuses.has(status)) {
    throw new ProjectMapApiError(
      "replay.reconstruction.status is unsupported",
      502,
      null,
    );
  }
  const gapsValue = reconstruction.gaps;
  if (!Array.isArray(gapsValue)) {
    throw new ProjectMapApiError(
      "replay.reconstruction.gaps is not an array",
      502,
      null,
    );
  }
  const liveEventSeq = numberField(
    meta,
    "live_event_seq",
    "replay.meta",
  );
  const atEventSeq = numberField(meta, "at_event_seq", "replay.meta");
  if (atEventSeq === 0 || atEventSeq > liveEventSeq) {
    throw new ProjectMapApiError(
      "replay.meta sequence range is invalid",
      502,
      null,
    );
  }
  const parsedTransactions = transactions.map(parseReplayTransaction);
  parsedTransactions.forEach((transaction, index) => {
    if (
      transaction.seq === 0 ||
      transaction.seq > atEventSeq ||
      (index > 0 &&
        transaction.seq <= parsedTransactions[index - 1].seq)
    ) {
      throw new ProjectMapApiError(
        "replay transactions are not strictly ordered within the selected range",
        502,
        null,
      );
    }
  });
  const state =
    value.state === null ? null : parseReplayState(value.state);
  if (status === "available" && !state) {
    throw new ProjectMapApiError(
      "available replay reconstruction has no state",
      502,
      null,
    );
  }
  return {
    schema_version: numberField(value, "schema_version", "replay"),
    meta: {
      live_event_seq: liveEventSeq,
      at_event_seq: atEventSeq,
      checkpoint_event_seq: nullableNumberField(
        meta,
        "checkpoint_event_seq",
        "replay.meta",
      ),
      earliest_replayable_seq: nullableNumberField(
        meta,
        "earliest_replayable_seq",
        "replay.meta",
      ),
      tree_revision: numberField(
        meta,
        "tree_revision",
        "replay.meta",
      ),
      projected_at: stringField(meta, "projected_at", "replay.meta"),
    },
    reconstruction: {
      status: status as ReplayResponse["reconstruction"]["status"],
      gaps: gapsValue.map((gap, index) => {
        if (!isRecord(gap)) {
          throw new ProjectMapApiError(
            `replay.reconstruction.gaps[${index}] is not an object`,
            502,
            null,
          );
        }
        const fromSeq = numberField(
          gap,
          "from_seq",
          `replay.reconstruction.gaps[${index}]`,
        );
        const toSeq = numberField(
          gap,
          "to_seq",
          `replay.reconstruction.gaps[${index}]`,
        );
        if (fromSeq === 0 || fromSeq > toSeq || toSeq > atEventSeq) {
          throw new ProjectMapApiError(
            `replay.reconstruction.gaps[${index}] has an invalid range`,
            502,
            null,
          );
        }
        return {
          from_seq: fromSeq,
          to_seq: toSeq,
          reason: stringField(
            gap,
            "reason",
            `replay.reconstruction.gaps[${index}]`,
          ),
        };
      }),
    },
    state,
    transactions: parsedTransactions,
  };
}

function parseSectionRecord(
  value: unknown,
  fields: readonly string[],
  context: string,
): Record<string, string> {
  if (!isRecord(value)) {
    throw new ProjectMapApiError(`${context} is not an object`, 502, null);
  }
  return Object.fromEntries(
    fields.map((field) => [field, stringField(value, field, context)]),
  );
}

export function parseBranchDetail(value: unknown): BranchDetail {
  if (!isRecord(value)) {
    throw new ProjectMapApiError("branch detail is not an object", 502, null);
  }
  const project = value.project;
  if (!isRecord(project)) {
    throw new ProjectMapApiError("branch project is not an object", 502, null);
  }
  return {
    schema_version: numberField(value, "schema_version", "branch"),
    tree_revision: numberField(value, "tree_revision", "branch"),
    state_event_seq: numberField(value, "state_event_seq", "branch"),
    narrative_revision: stringField(value, "narrative_revision", "branch"),
    tree_editing: booleanField(value, "tree_editing", "branch"),
    projected_at: stringField(value, "projected_at", "branch"),
    health: parseHealth(value.health),
    project: {
      stage: stringField(project, "stage", "project"),
      current_branch: stringField(project, "current_branch", "project"),
      topology_source: stringField(project, "topology_source", "project"),
    },
    branch: parseNode(value.branch, 0),
    task_plan: parseSectionRecord(
      value.task_plan,
      [
        "scope",
        "acceptance",
        "local_steps",
        "out_of_scope",
        "dependencies",
        "branch_intake_gate",
      ],
      "task_plan",
    ) as unknown as BranchDetail["task_plan"],
    progress: parseSectionRecord(
      value.progress,
      ["current_reality", "recent_work", "open_issues", "exit_notes"],
      "progress",
    ) as unknown as BranchDetail["progress"],
    findings: parseSectionRecord(
      value.findings,
      ["decisions", "interface_or_contract_effects", "risks_and_unknowns"],
      "findings",
    ) as unknown as BranchDetail["findings"],
    verification: parseSectionRecord(
      value.verification,
      ["status", "evidence", "coverage_gap"],
      "verification",
    ) as unknown as BranchDetail["verification"],
  };
}

export function parseInvalidation(value: unknown): ProjectMapInvalidation {
  if (!isRecord(value)) {
    throw new ProjectMapApiError("invalidation is not an object", 502, null);
  }
  const kind = stringField(value, "kind", "invalidation");
  const changes = stringArray(value, "changes", "invalidation");
  if (
    kind !== "project_map.invalidated" ||
    changes.some((change) => !invalidationCategories.has(change))
  ) {
    throw new ProjectMapApiError(
      "invalidation has unsupported semantics",
      502,
      null,
    );
  }
  return {
    schema_version: numberField(value, "schema_version", "invalidation"),
    kind,
    changes: changes as ProjectMapInvalidation["changes"],
    tree_revision: numberField(value, "tree_revision", "invalidation"),
    state_event_seq: numberField(value, "state_event_seq", "invalidation"),
    narrative_revision: stringField(
      value,
      "narrative_revision",
      "invalidation",
    ),
  };
}

async function requestJson(
  path: string,
  signal?: AbortSignal,
): Promise<unknown> {
  const response = await fetch(path, {
    method: "GET",
    cache: "no-store",
    headers: { Accept: "application/json" },
    signal,
  });
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    throw new ProjectMapApiError(
      `Project Map returned non-JSON data (${response.status})`,
      response.status,
      null,
    );
  }
  if (!response.ok) {
    const failure = isRecord(body) ? (body as ApiFailure) : {};
    throw new ProjectMapApiError(
      failure.error ||
        failure.health?.message ||
        `Project Map request failed (${response.status})`,
      response.status,
      failure.health?.status ?? null,
    );
  }
  return body;
}

export async function fetchProjection(
  signal?: AbortSignal,
): Promise<ProjectMapProjection> {
  return parseProjection(await requestJson("/api/project-map", signal));
}

export async function fetchBranchDetail(
  id: string,
  signal?: AbortSignal,
): Promise<BranchDetail> {
  const query = new URLSearchParams({ id });
  return parseBranchDetail(
    await requestJson(`/api/project-map/branch?${query}`, signal),
  );
}

export interface ReplayRequestOptions {
  at?: number;
  after?: number;
  branch?: string;
}

export async function fetchReplay(
  options: ReplayRequestOptions = {},
  signal?: AbortSignal,
): Promise<ReplayResponse> {
  const query = new URLSearchParams();
  if (options.at !== undefined) {
    query.set("at", String(options.at));
  }
  if (options.after !== undefined) {
    query.set("after", String(options.after));
  }
  if (options.branch) {
    query.set("branch", options.branch);
  }
  const suffix = query.size ? `?${query}` : "";
  return parseReplayResponse(
    await requestJson(`/api/project-map/replay${suffix}`, signal),
  );
}
