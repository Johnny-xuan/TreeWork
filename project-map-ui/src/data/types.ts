export type LifecycleStatus =
  | "pending"
  | "in_progress"
  | "paused"
  | "complete"
  | "aborted";

export type VerificationStatus =
  | "unverified"
  | "partial"
  | "verified"
  | "failed";

export type ReadinessStatus =
  | "active"
  | "ready"
  | "waiting"
  | "paused"
  | "complete"
  | "aborted";

export interface ProjectMapHealth {
  status: "ok" | "warning" | "degraded" | "unavailable" | string;
  message: string;
}

export interface ProjectMapProject {
  stage: string;
  current_branch: string;
  topology_source: string;
}

export interface ProjectMapNode {
  id: string;
  parent: string;
  order: number;
  title: string;
  purpose: string;
  spec: string | null;
  status: LifecycleStatus;
  verification: VerificationStatus;
  status_reason: string;
  is_current: boolean;
  readiness: ReadinessStatus;
  depends_on: string[];
  child_count: number;
}

export interface ProjectMapDependency {
  from: string;
  to: string;
  satisfied: boolean;
}

export interface ProjectMapProjection {
  schema_version: number;
  tree_revision: number;
  state_event_seq: number;
  narrative_revision: string;
  tree_editing: boolean;
  projected_at: string;
  health: ProjectMapHealth;
  project: ProjectMapProject;
  nodes: ProjectMapNode[];
  dependencies: ProjectMapDependency[];
}

export interface TaskPlanSections {
  scope: string;
  acceptance: string;
  local_steps: string;
  out_of_scope: string;
  dependencies: string;
  branch_intake_gate: string;
}

export interface ProgressSections {
  current_reality: string;
  recent_work: string;
  open_issues: string;
  exit_notes: string;
}

export interface FindingsSections {
  decisions: string;
  interface_or_contract_effects: string;
  risks_and_unknowns: string;
}

export interface VerificationSections {
  status: string;
  evidence: string;
  coverage_gap: string;
}

export interface BranchDetail {
  schema_version: number;
  tree_revision: number;
  state_event_seq: number;
  narrative_revision: string;
  tree_editing: boolean;
  projected_at: string;
  health: ProjectMapHealth;
  project: ProjectMapProject;
  branch: ProjectMapNode;
  task_plan: TaskPlanSections;
  progress: ProgressSections;
  findings: FindingsSections;
  verification: VerificationSections;
}

export type InvalidationCategory =
  | "topology"
  | "state"
  | "narrative"
  | "events"
  | "health";

export interface ProjectMapInvalidation {
  schema_version: number;
  kind: "project_map.invalidated";
  changes: InvalidationCategory[];
  tree_revision: number;
  state_event_seq: number;
  narrative_revision: string;
}

export type ReplayReconstructionStatus =
  | "available"
  | "partial"
  | "unavailable";

export type ReplaySpeed = 0.5 | 1 | 2 | 4;

export interface ReplayMeta {
  live_event_seq: number;
  at_event_seq: number;
  checkpoint_event_seq: number | null;
  earliest_replayable_seq: number | null;
  tree_revision: number;
  projected_at: string;
}

export interface ReplayGap {
  from_seq: number;
  to_seq: number;
  reason: string;
}

export interface ReplayState {
  tree_editing: boolean;
  project: ProjectMapProject;
  nodes: ProjectMapNode[];
  dependencies: ProjectMapDependency[];
}

export interface ReplayTransaction {
  seq: number;
  time: string;
  type: string;
  subject: string;
  message: string;
  tree_revision: number | null;
  affected_subjects: string[];
  changes: unknown;
  replayable: boolean;
  replayability_reason: string | null;
}

export interface ReplayResponse {
  schema_version: number;
  meta: ReplayMeta;
  reconstruction: {
    status: ReplayReconstructionStatus;
    gaps: ReplayGap[];
  };
  state: ReplayState | null;
  transactions: ReplayTransaction[];
}

export interface ApiFailure {
  ok?: false;
  error?: string;
  schema_version?: number;
  health?: ProjectMapHealth;
}
