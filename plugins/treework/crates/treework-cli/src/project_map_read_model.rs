use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(test)]
use crate::branch_artifacts::HIERARCHICAL_LAYOUT;
use crate::branch_artifacts::{BranchArtifactLayout, BranchArtifactNode, LEGACY_FLAT_LAYOUT};

const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_DOCUMENT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_EVENT_TAIL_BYTES: u64 = 1024 * 1024;
const DEFAULT_STABILITY_DELAY: Duration = Duration::from_millis(25);
const CONTROL_DESCRIPTOR: &str = "treework/control.json";
const WORKTREE_BRANCH_DESCRIPTOR: &str = "treework-branch.json";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectMapHealth {
    pub status: String,
    pub message: String,
}

impl ProjectMapHealth {
    fn ok() -> Self {
        Self {
            status: "ok".to_string(),
            message: String::new(),
        }
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: "unavailable".to_string(),
            message: message.into(),
        }
    }

    fn degraded(message: impl Into<String>) -> Self {
        Self {
            status: "degraded".to_string(),
            message: message.into(),
        }
    }

    fn warning(message: impl Into<String>) -> Self {
        Self {
            status: "warning".to_string(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectMapProject {
    pub stage: String,
    pub current_branch: String,
    pub topology_source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectMapNode {
    pub id: String,
    pub parent: String,
    pub order: usize,
    pub title: String,
    pub purpose: String,
    pub spec: Option<String>,
    pub status: String,
    pub verification: String,
    pub status_reason: String,
    pub is_current: bool,
    pub readiness: String,
    pub depends_on: Vec<String>,
    pub child_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectMapDependency {
    pub from: String,
    pub to: String,
    pub satisfied: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectMapProjection {
    pub schema_version: u32,
    pub tree_revision: u64,
    pub state_event_seq: u64,
    pub narrative_revision: String,
    pub tree_editing: bool,
    pub projected_at: String,
    pub health: ProjectMapHealth,
    pub project: ProjectMapProject,
    pub nodes: Vec<ProjectMapNode>,
    pub dependencies: Vec<ProjectMapDependency>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct TaskPlanSections {
    pub scope: String,
    pub acceptance: String,
    pub local_steps: String,
    pub out_of_scope: String,
    pub dependencies: String,
    pub branch_intake_gate: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProgressSections {
    pub current_reality: String,
    pub recent_work: String,
    pub open_issues: String,
    pub exit_notes: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct FindingsSections {
    pub decisions: String,
    pub interface_or_contract_effects: String,
    pub risks_and_unknowns: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct VerificationSections {
    pub status: String,
    pub evidence: String,
    pub coverage_gap: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct BranchDetail {
    pub schema_version: u32,
    pub tree_revision: u64,
    pub state_event_seq: u64,
    pub narrative_revision: String,
    pub tree_editing: bool,
    pub projected_at: String,
    pub health: ProjectMapHealth,
    pub project: ProjectMapProject,
    pub branch: ProjectMapNode,
    pub task_plan: TaskPlanSections,
    pub progress: ProgressSections,
    pub findings: FindingsSections,
    pub verification: VerificationSections,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct UnavailableProjection {
    pub schema_version: u32,
    pub health: ProjectMapHealth,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct ProjectMapInvalidation {
    pub schema_version: u32,
    pub kind: String,
    pub changes: Vec<String>,
    pub tree_revision: u64,
    pub state_event_seq: u64,
    pub narrative_revision: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum ChangeCategory {
    Topology,
    State,
    Narrative,
    Events,
    Health,
}

impl ChangeCategory {
    fn as_str(self) -> &'static str {
        match self {
            Self::Topology => "topology",
            Self::State => "state",
            Self::Narrative => "narrative",
            Self::Events => "events",
            Self::Health => "health",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RefreshReport {
    pub invalidation: Option<ProjectMapInvalidation>,
}

#[derive(Clone, Debug)]
pub(crate) enum BranchLookupError {
    Invalid(String),
    Unknown(String),
    Unavailable(UnavailableProjection),
}

#[derive(Debug, Clone)]
struct ReadModelError(String);

impl fmt::Display for ReadModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

type ReadResult<T> = Result<T, ReadModelError>;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictProject {
    schema_version: String,
    stage: String,
    current_branch: String,
    #[serde(default = "default_artifact_layout_version")]
    artifact_layout_version: u32,
    last_event_seq: u64,
    tree_revision: u64,
    tree_editing: Option<StrictTreeEditing>,
    tree_hash: String,
    last_sync: String,
}

fn default_artifact_layout_version() -> u32 {
    LEGACY_FLAT_LAYOUT
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictTreeEditing {
    mode: String,
    base_tree_revision: u64,
    base_event_seq: u64,
    base_state_hash: String,
    opened_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictBranchState {
    branches: Vec<StrictBranch>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictBranch {
    path: String,
    parent: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    scope: Option<StrictBranchScope>,
    #[serde(default)]
    intake_rationale: String,
    status: String,
    verification_status: String,
    sync_status: String,
    #[serde(default)]
    isolation: Option<StrictIsolation>,
    #[serde(default)]
    status_reason: String,
    #[serde(default)]
    last_sync: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictBranchScope {
    accepts: Vec<String>,
    excludes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictIsolation {
    mode: String,
    workspace_path: String,
    git_branch: String,
    managed_by_treework: bool,
    created_at: String,
    last_entered_at: String,
    last_status: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictGraphState {
    edges: Vec<StrictEdge>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictEdge {
    id: String,
    from: String,
    to: String,
    kind: String,
    user_label: String,
    interpreted_relation: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictAcceptedTree {
    schema_version: u32,
    revision: u64,
    source_hash: String,
    state_hash: String,
    accepted_at: String,
    root: String,
    nodes: Vec<StrictAcceptedTreeNode>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct StrictAcceptedTreeNode {
    id: String,
    parent: String,
    title: String,
    purpose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    spec: Option<String>,
    sibling_order: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    depends_on: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictControlDescriptor {
    version: u32,
    project_id: String,
    control_root: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictWorktreeBinding {
    version: u32,
    project_id: String,
    branch: String,
    workspace: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BranchNarrative {
    task_plan: TaskPlanSections,
    progress: ProgressSections,
    findings: FindingsSections,
    verification: VerificationSections,
    warnings: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NarrativeSet {
    by_branch: HashMap<String, BranchNarrative>,
    raw_documents: Vec<(String, String, String)>,
    managed_watch_roots: BTreeSet<PathBuf>,
}

#[derive(Clone, Debug)]
struct ProjectMapSnapshot {
    projection: ProjectMapProjection,
    narratives: HashMap<String, BranchNarrative>,
    managed_watch_roots: BTreeSet<PathBuf>,
}

struct StoreState {
    snapshot: Option<ProjectMapSnapshot>,
    health: ProjectMapHealth,
}

pub(crate) struct ProjectMapStore {
    root: PathBuf,
    stability_delay: Duration,
    refresh_guard: Mutex<()>,
    state: RwLock<StoreState>,
}

impl ProjectMapStore {
    pub(crate) fn new(root: PathBuf) -> Self {
        Self::with_stability_delay(root, DEFAULT_STABILITY_DELAY)
    }

    fn with_stability_delay(root: PathBuf, stability_delay: Duration) -> Self {
        let root = fs::canonicalize(&root).unwrap_or(root);
        Self {
            root,
            stability_delay,
            refresh_guard: Mutex::new(()),
            state: RwLock::new(StoreState {
                snapshot: None,
                health: ProjectMapHealth::unavailable(
                    "Project Map has not loaded a coherent accepted-state projection.",
                ),
            }),
        }
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn refresh(&self) -> RefreshReport {
        let _refresh = self
            .refresh_guard
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match load_coherent_snapshot(&self.root, self.stability_delay) {
            Ok(next) => self.accept_snapshot(next),
            Err(error) => self.reject_snapshot(error.to_string()),
        }
    }

    pub(crate) fn projection(&self) -> Result<ProjectMapProjection, UnavailableProjection> {
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(snapshot) = &state.snapshot else {
            return Err(UnavailableProjection {
                schema_version: 1,
                health: state.health.clone(),
            });
        };
        let mut projection = snapshot.projection.clone();
        projection.health = state.health.clone();
        Ok(projection)
    }

    pub(crate) fn branch_detail(&self, id: &str) -> Result<BranchDetail, BranchLookupError> {
        validate_branch_id(id).map_err(|error| BranchLookupError::Invalid(error.0))?;
        let state = self
            .state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(snapshot) = &state.snapshot else {
            return Err(BranchLookupError::Unavailable(UnavailableProjection {
                schema_version: 1,
                health: state.health.clone(),
            }));
        };
        let branch = snapshot
            .projection
            .nodes
            .iter()
            .find(|node| node.id == id)
            .cloned()
            .ok_or_else(|| BranchLookupError::Unknown(id.to_string()))?;
        let narrative = snapshot
            .narratives
            .get(id)
            .cloned()
            .unwrap_or_else(empty_narrative);
        let health = branch_detail_health(&state.health, &narrative.warnings);
        Ok(BranchDetail {
            schema_version: snapshot.projection.schema_version,
            tree_revision: snapshot.projection.tree_revision,
            state_event_seq: snapshot.projection.state_event_seq,
            narrative_revision: snapshot.projection.narrative_revision.clone(),
            tree_editing: snapshot.projection.tree_editing,
            projected_at: snapshot.projection.projected_at.clone(),
            health,
            project: snapshot.projection.project.clone(),
            branch,
            task_plan: narrative.task_plan,
            progress: narrative.progress,
            findings: narrative.findings,
            verification: narrative.verification,
        })
    }

    pub(crate) fn managed_watch_roots(&self) -> BTreeSet<PathBuf> {
        self.state
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot
            .as_ref()
            .map(|snapshot| snapshot.managed_watch_roots.clone())
            .unwrap_or_default()
    }

    fn accept_snapshot(&self, mut next: ProjectMapSnapshot) -> RefreshReport {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous_health = state.health.clone();
        let mut changes = BTreeSet::new();
        if let Some(previous) = &state.snapshot {
            classify_snapshot_changes(previous, &next, &mut changes);
        } else {
            changes.extend([
                ChangeCategory::Topology,
                ChangeCategory::State,
                ChangeCategory::Narrative,
                ChangeCategory::Events,
            ]);
        }
        state.health = ProjectMapHealth::ok();
        next.projection.health = ProjectMapHealth::ok();
        if previous_health != state.health {
            changes.insert(ChangeCategory::Health);
        }
        state.snapshot = Some(next);
        RefreshReport {
            invalidation: invalidation_from_state(&state, changes),
        }
    }

    fn reject_snapshot(&self, error: String) -> RefreshReport {
        let mut state = self
            .state
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let next_health = if state.snapshot.is_some() {
            ProjectMapHealth::degraded(error.clone())
        } else {
            ProjectMapHealth::unavailable(error.clone())
        };
        let changed = state.health != next_health;
        state.health = next_health;
        let changes = if changed {
            BTreeSet::from([ChangeCategory::Health])
        } else {
            BTreeSet::new()
        };
        RefreshReport {
            invalidation: invalidation_from_state(&state, changes),
        }
    }
}

fn classify_snapshot_changes(
    previous: &ProjectMapSnapshot,
    next: &ProjectMapSnapshot,
    changes: &mut BTreeSet<ChangeCategory>,
) {
    if !topology_equal(&previous.projection, &next.projection) {
        changes.insert(ChangeCategory::Topology);
    }
    if !visible_state_equal(&previous.projection, &next.projection) {
        changes.insert(ChangeCategory::State);
    }
    if previous.projection.narrative_revision != next.projection.narrative_revision {
        changes.insert(ChangeCategory::Narrative);
    }
    if previous.projection.state_event_seq != next.projection.state_event_seq {
        changes.insert(ChangeCategory::Events);
    }
}

fn topology_equal(left: &ProjectMapProjection, right: &ProjectMapProjection) -> bool {
    left.tree_revision == right.tree_revision
        && left.project.topology_source == right.project.topology_source
        && left.nodes.len() == right.nodes.len()
        && left.dependencies.len() == right.dependencies.len()
        && left.nodes.iter().zip(&right.nodes).all(|(a, b)| {
            a.id == b.id
                && a.parent == b.parent
                && a.order == b.order
                && a.title == b.title
                && a.purpose == b.purpose
                && a.spec == b.spec
                && a.depends_on == b.depends_on
                && a.child_count == b.child_count
        })
        && left
            .dependencies
            .iter()
            .zip(&right.dependencies)
            .all(|(a, b)| a.from == b.from && a.to == b.to)
}

fn visible_state_equal(left: &ProjectMapProjection, right: &ProjectMapProjection) -> bool {
    left.tree_editing == right.tree_editing
        && left.project.stage == right.project.stage
        && left.project.current_branch == right.project.current_branch
        && left.nodes.len() == right.nodes.len()
        && left.nodes.iter().zip(&right.nodes).all(|(a, b)| {
            a.id == b.id
                && a.status == b.status
                && a.verification == b.verification
                && a.status_reason == b.status_reason
                && a.is_current == b.is_current
                && a.readiness == b.readiness
        })
        && left
            .dependencies
            .iter()
            .zip(&right.dependencies)
            .all(|(a, b)| a.satisfied == b.satisfied)
}

fn invalidation_from_state(
    state: &StoreState,
    changes: BTreeSet<ChangeCategory>,
) -> Option<ProjectMapInvalidation> {
    if changes.is_empty() {
        return None;
    }
    let (tree_revision, state_event_seq, narrative_revision) = state
        .snapshot
        .as_ref()
        .map(|snapshot| {
            (
                snapshot.projection.tree_revision,
                snapshot.projection.state_event_seq,
                snapshot.projection.narrative_revision.clone(),
            )
        })
        .unwrap_or_else(|| (0, 0, empty_narrative_revision()));
    Some(ProjectMapInvalidation {
        schema_version: 1,
        kind: "project_map.invalidated".to_string(),
        changes: changes
            .into_iter()
            .map(|change| change.as_str().to_string())
            .collect(),
        tree_revision,
        state_event_seq,
        narrative_revision,
    })
}

fn load_coherent_snapshot(
    root: &Path,
    stability_delay: Duration,
) -> ReadResult<ProjectMapSnapshot> {
    load_coherent_snapshot_with_hook(root, stability_delay, || {})
}

fn load_coherent_snapshot_with_hook<F>(
    root: &Path,
    stability_delay: Duration,
    after_marker_a: F,
) -> ReadResult<ProjectMapSnapshot>
where
    F: FnOnce(),
{
    load_coherent_snapshot_with_hooks(root, stability_delay, after_marker_a, || {})
}

fn load_coherent_snapshot_with_hooks<F, G>(
    root: &Path,
    stability_delay: Duration,
    after_marker_a: F,
    after_narratives_a: G,
) -> ReadResult<ProjectMapSnapshot>
where
    F: FnOnce(),
    G: FnOnce(),
{
    let root = canonical_directory(root, "project root")?;
    let treework = canonical_directory(&root.join(".TreeWork"), ".TreeWork directory")?;
    ensure_lock_absent(&root)?;
    let project_path = treework.join("state/project.json");
    let marker_a: StrictProject = read_strict_json(&project_path)?;
    after_marker_a();

    let branches: StrictBranchState = read_strict_json(&treework.join("state/branches.json"))?;
    let graph: StrictGraphState = read_strict_json(&treework.join("state/graph.json"))?;
    let tree = if marker_a.tree_revision == 0 {
        None
    } else {
        Some(read_strict_json::<StrictAcceptedTree>(
            &treework.join("state/tree.json"),
        )?)
    };
    let event_tail = read_event_tail_seq(&treework.join("events.jsonl"))?;
    let marker_b: StrictProject = read_strict_json(&project_path)?;
    ensure_lock_absent(&root)?;
    if marker_a != marker_b {
        return Err(ReadModelError(
            "publication marker changed during coherent read".to_string(),
        ));
    }
    validate_accepted_tuple(
        &marker_b,
        tree.as_ref(),
        &branches.branches,
        &graph,
        event_tail,
    )?;

    let narratives_a = read_narratives(&root, &treework, &marker_b, &branches.branches)?;
    after_narratives_a();
    if !stability_delay.is_zero() {
        thread::sleep(stability_delay);
    }
    let narratives_b = read_narratives(&root, &treework, &marker_b, &branches.branches)?;
    if narratives_a != narratives_b {
        return Err(ReadModelError(
            "branch documents changed during stable narrative read".to_string(),
        ));
    }
    ensure_lock_absent(&root)?;
    let marker_c: StrictProject = read_strict_json(&project_path)?;
    if marker_b != marker_c {
        return Err(ReadModelError(
            "publication marker changed while branch documents were read".to_string(),
        ));
    }

    compose_snapshot(&marker_c, tree.as_ref(), &branches.branches, narratives_b)
}

fn validate_accepted_tuple(
    project: &StrictProject,
    tree: Option<&StrictAcceptedTree>,
    branches: &[StrictBranch],
    graph: &StrictGraphState,
    event_tail: u64,
) -> ReadResult<()> {
    validate_project(project)?;
    let branch_by_id = validate_branches(branches)?;
    if !branch_by_id.contains_key(project.current_branch.as_str()) {
        return Err(ReadModelError(format!(
            "current branch `{}` is absent from accepted branch state",
            project.current_branch
        )));
    }
    if event_tail != project.last_event_seq {
        return Err(ReadModelError(format!(
            "event tail sequence {} does not match publication marker {}",
            event_tail, project.last_event_seq
        )));
    }

    if project.tree_revision == 0 {
        if !project.tree_hash.is_empty() {
            return Err(ReadModelError(
                "revision-zero project has a non-empty tree hash".to_string(),
            ));
        }
        if !branch_by_id.contains_key("root") {
            return Err(ReadModelError(
                "revision-zero project has no accepted root branch".to_string(),
            ));
        }
        validate_graph_endpoints(graph, &branch_by_id)?;
        return Ok(());
    }

    let tree = tree.ok_or_else(|| {
        ReadModelError("accepted tree revision has no state/tree.json snapshot".to_string())
    })?;
    validate_tree(project, tree, &branch_by_id)?;
    validate_graph(graph, tree, &branch_by_id)
}

fn validate_project(project: &StrictProject) -> ReadResult<()> {
    if project.schema_version != "0.1" {
        return Err(ReadModelError(format!(
            "unsupported project schema version `{}`",
            project.schema_version
        )));
    }
    if !matches!(
        project.stage.as_str(),
        "alignment" | "build_tree" | "work_tree"
    ) {
        return Err(ReadModelError(format!(
            "unsupported project stage `{}`",
            project.stage
        )));
    }
    validate_branch_id(&project.current_branch)?;
    if project.last_sync.trim().is_empty() {
        return Err(ReadModelError(
            "project last_sync must not be empty".to_string(),
        ));
    }
    if let Some(editing) = &project.tree_editing {
        if !matches!(editing.mode.as_str(), "start" | "update") {
            return Err(ReadModelError(format!(
                "unsupported tree editing mode `{}`",
                editing.mode
            )));
        }
        if editing.base_tree_revision > project.tree_revision
            || editing.base_event_seq > project.last_event_seq
            || editing.base_state_hash.trim().is_empty()
            || editing.opened_at.trim().is_empty()
        {
            return Err(ReadModelError(
                "tree editing session is inconsistent with project marker".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_branches(branches: &[StrictBranch]) -> ReadResult<HashMap<&str, &StrictBranch>> {
    if branches.is_empty() {
        return Err(ReadModelError(
            "accepted branch state contains no branches".to_string(),
        ));
    }
    let mut by_id = HashMap::new();
    for branch in branches {
        validate_branch_id(&branch.path)?;
        if by_id.insert(branch.path.as_str(), branch).is_some() {
            return Err(ReadModelError(format!(
                "duplicate accepted branch `{}`",
                branch.path
            )));
        }
        if !branch.parent.is_empty() {
            validate_branch_id(&branch.parent)?;
        }
        if !matches!(
            branch.status.as_str(),
            "pending" | "in_progress" | "paused" | "complete" | "aborted"
        ) {
            return Err(ReadModelError(format!(
                "branch `{}` has unsupported status `{}`",
                branch.path, branch.status
            )));
        }
        if !matches!(
            branch.verification_status.as_str(),
            "unverified" | "partial" | "verified" | "failed"
        ) {
            return Err(ReadModelError(format!(
                "branch `{}` has unsupported verification `{}`",
                branch.path, branch.verification_status
            )));
        }
        if !matches!(branch.sync_status.as_str(), "clean" | "dirty" | "stale") {
            return Err(ReadModelError(format!(
                "branch `{}` has unsupported sync status `{}`",
                branch.path, branch.sync_status
            )));
        }
        if branch
            .last_sync
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ReadModelError(format!(
                "branch `{}` has empty last_sync",
                branch.path
            )));
        }
        if let Some(scope) = &branch.scope {
            if scope.accepts.iter().any(|value| value.trim().is_empty())
                || scope.excludes.iter().any(|value| value.trim().is_empty())
            {
                return Err(ReadModelError(format!(
                    "branch `{}` has an empty scope entry",
                    branch.path
                )));
            }
        }
        if let Some(isolation) = &branch.isolation {
            if isolation.managed_by_treework && isolation.workspace_path.trim().is_empty() {
                return Err(ReadModelError(format!(
                    "branch `{}` has managed isolation without a workspace path",
                    branch.path
                )));
            }
        }
    }
    Ok(by_id)
}

fn validate_tree(
    project: &StrictProject,
    tree: &StrictAcceptedTree,
    branches: &HashMap<&str, &StrictBranch>,
) -> ReadResult<()> {
    if tree.schema_version != 1 {
        return Err(ReadModelError(format!(
            "unsupported accepted Tree schema version {}",
            tree.schema_version
        )));
    }
    if tree.revision != project.tree_revision {
        return Err(ReadModelError(format!(
            "accepted Tree revision {} does not match project revision {}",
            tree.revision, project.tree_revision
        )));
    }
    if tree.source_hash != project.tree_hash {
        return Err(ReadModelError(
            "accepted Tree source hash does not match project marker".to_string(),
        ));
    }
    if tree.root != "root" || tree.nodes.is_empty() {
        return Err(ReadModelError(
            "accepted Tree must contain root topology".to_string(),
        ));
    }
    if tree.source_hash.trim().is_empty()
        || tree.state_hash.trim().is_empty()
        || tree.accepted_at.trim().is_empty()
    {
        return Err(ReadModelError(
            "accepted Tree hashes and acceptance time must not be empty".to_string(),
        ));
    }
    let computed_hash = strict_tree_state_hash(tree)?;
    if tree.state_hash != computed_hash {
        return Err(ReadModelError(format!(
            "accepted Tree state hash is invalid: expected {}, found {}",
            computed_hash, tree.state_hash
        )));
    }

    let mut nodes = HashMap::new();
    let mut sibling_orders: HashMap<&str, HashSet<usize>> = HashMap::new();
    for node in &tree.nodes {
        validate_branch_id(&node.id)?;
        if nodes.insert(node.id.as_str(), node).is_some() {
            return Err(ReadModelError(format!(
                "accepted Tree contains duplicate node `{}`",
                node.id
            )));
        }
        if node.id == "root" {
            if !node.parent.is_empty() || node.sibling_order != 0 {
                return Err(ReadModelError(
                    "accepted root node has invalid parent or order".to_string(),
                ));
            }
        } else {
            validate_branch_id(&node.parent)?;
        }
        if node.title.trim().is_empty() || node.purpose.trim().is_empty() {
            return Err(ReadModelError(format!(
                "accepted Tree node `{}` has empty title or purpose",
                node.id
            )));
        }
        if let Some(spec) = &node.spec {
            validate_spec_path(spec)?;
        }
        if !sibling_orders
            .entry(node.parent.as_str())
            .or_default()
            .insert(node.sibling_order)
        {
            return Err(ReadModelError(format!(
                "accepted Tree has duplicate sibling order {} under `{}`",
                node.sibling_order, node.parent
            )));
        }
        let mut dependencies = HashSet::new();
        for dependency in &node.depends_on {
            validate_branch_id(dependency)?;
            if dependency == &node.id || !dependencies.insert(dependency.as_str()) {
                return Err(ReadModelError(format!(
                    "accepted Tree node `{}` has an invalid dependency `{}`",
                    node.id, dependency
                )));
            }
        }
    }
    if nodes.len() != branches.len()
        || nodes.keys().any(|id| !branches.contains_key(id))
        || branches.keys().any(|id| !nodes.contains_key(id))
    {
        return Err(ReadModelError(
            "accepted Tree and branch state contain different branch IDs".to_string(),
        ));
    }
    for node in &tree.nodes {
        if node.id != "root" && !nodes.contains_key(node.parent.as_str()) {
            return Err(ReadModelError(format!(
                "accepted Tree node `{}` references missing parent `{}`",
                node.id, node.parent
            )));
        }
        for dependency in &node.depends_on {
            if !nodes.contains_key(dependency.as_str()) {
                return Err(ReadModelError(format!(
                    "accepted Tree node `{}` references missing dependency `{}`",
                    node.id, dependency
                )));
            }
        }
    }
    validate_parent_cycles(&nodes)?;
    validate_dependency_cycles(&nodes)
}

fn validate_parent_cycles(nodes: &HashMap<&str, &StrictAcceptedTreeNode>) -> ReadResult<()> {
    for id in nodes.keys().copied() {
        let mut seen = HashSet::new();
        let mut cursor = id;
        loop {
            if !seen.insert(cursor) {
                return Err(ReadModelError(format!(
                    "accepted Tree parent cycle includes `{}`",
                    id
                )));
            }
            let node = nodes.get(cursor).ok_or_else(|| {
                ReadModelError(format!(
                    "accepted Tree parent chain for `{}` references missing `{}`",
                    id, cursor
                ))
            })?;
            if node.parent.is_empty() {
                if cursor != "root" {
                    return Err(ReadModelError(format!(
                        "accepted Tree node `{}` is disconnected from root",
                        id
                    )));
                }
                break;
            }
            cursor = node.parent.as_str();
        }
    }
    Ok(())
}

fn validate_dependency_cycles(nodes: &HashMap<&str, &StrictAcceptedTreeNode>) -> ReadResult<()> {
    fn visit<'a>(
        id: &'a str,
        nodes: &HashMap<&'a str, &'a StrictAcceptedTreeNode>,
        marks: &mut HashMap<&'a str, u8>,
    ) -> ReadResult<()> {
        marks.insert(id, 1);
        let node = nodes.get(id).ok_or_else(|| {
            ReadModelError(format!("accepted Tree dependency map is missing `{}`", id))
        })?;
        for dependency in &node.depends_on {
            match marks.get(dependency.as_str()).copied().unwrap_or_default() {
                0 => visit(dependency, nodes, marks)?,
                1 => {
                    return Err(ReadModelError(format!(
                        "accepted Tree dependency cycle includes `{}`",
                        dependency
                    )));
                }
                _ => {}
            }
        }
        marks.insert(id, 2);
        Ok(())
    }

    let mut marks = HashMap::new();
    for id in nodes.keys().copied() {
        if marks.get(id).copied().unwrap_or_default() == 0 {
            visit(id, nodes, &mut marks)?;
        }
    }
    Ok(())
}

fn validate_graph_endpoints(
    graph: &StrictGraphState,
    branches: &HashMap<&str, &StrictBranch>,
) -> ReadResult<()> {
    let mut ids = HashSet::new();
    for edge in &graph.edges {
        if edge.id.trim().is_empty() || !ids.insert(edge.id.as_str()) {
            return Err(ReadModelError(
                "accepted graph contains an empty or duplicate edge ID".to_string(),
            ));
        }
        if !branches.contains_key(edge.from.as_str()) || !branches.contains_key(edge.to.as_str()) {
            return Err(ReadModelError(format!(
                "accepted graph edge `{}` references a missing branch",
                edge.id
            )));
        }
    }
    Ok(())
}

fn validate_graph(
    graph: &StrictGraphState,
    tree: &StrictAcceptedTree,
    branches: &HashMap<&str, &StrictBranch>,
) -> ReadResult<()> {
    validate_graph_endpoints(graph, branches)?;
    let mut expected = HashSet::new();
    for node in &tree.nodes {
        if !node.parent.is_empty() {
            expected.insert((node.parent.as_str(), node.id.as_str(), "parent_of"));
        }
        for dependency in &node.depends_on {
            expected.insert((node.id.as_str(), dependency.as_str(), "depends_on"));
        }
    }
    let actual: HashSet<(&str, &str, &str)> = graph
        .edges
        .iter()
        .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.kind.as_str()))
        .collect();
    if actual.len() != graph.edges.len() {
        return Err(ReadModelError(
            "accepted graph contains a duplicate relation".to_string(),
        ));
    }
    if actual != expected {
        return Err(ReadModelError(
            "accepted graph does not match accepted Tree relations".to_string(),
        ));
    }
    Ok(())
}

fn strict_tree_state_hash(tree: &StrictAcceptedTree) -> ReadResult<String> {
    let value = json!({
        "schema_version": tree.schema_version,
        "revision": tree.revision,
        "source_hash": tree.source_hash,
        "root": tree.root,
        "nodes": tree.nodes,
    });
    let serialized = serde_json::to_string(&value)
        .map_err(|error| ReadModelError(format!("cannot hash accepted Tree: {}", error)))?;
    Ok(stable_hash_str(&serialized))
}

fn read_event_tail_seq(path: &Path) -> ReadResult<u64> {
    let metadata = fs::metadata(path).map_err(|error| {
        ReadModelError(format!(
            "cannot read event log metadata {}: {}",
            path.display(),
            error
        ))
    })?;
    let length = metadata.len();
    if length == 0 {
        return Ok(0);
    }
    let tail_start = length.saturating_sub(MAX_EVENT_TAIL_BYTES);
    let read_start = tail_start.saturating_sub(1);
    let read_length = length - read_start;
    let mut file = fs::File::open(path).map_err(|error| {
        ReadModelError(format!(
            "cannot open event log {}: {}",
            path.display(),
            error
        ))
    })?;
    file.seek(SeekFrom::Start(read_start)).map_err(|error| {
        ReadModelError(format!(
            "cannot seek event log {}: {}",
            path.display(),
            error
        ))
    })?;
    let mut bytes = vec![0; read_length as usize];
    file.read_exact(&mut bytes).map_err(|error| {
        ReadModelError(format!(
            "cannot read event log tail {}: {}",
            path.display(),
            error
        ))
    })?;
    if !bytes.ends_with(b"\n") {
        return Err(ReadModelError(
            "event log ends with a partial JSONL record".to_string(),
        ));
    }
    let tail = if tail_start == 0 {
        bytes.as_slice()
    } else {
        let candidate = &bytes[1..];
        if bytes[0] == b'\n' {
            candidate
        } else {
            let boundary = candidate
                .iter()
                .position(|byte| *byte == b'\n')
                .ok_or_else(|| {
                    ReadModelError("final event record exceeds the bounded tail window".to_string())
                })?;
            &candidate[boundary + 1..]
        }
    };
    let Some(line) = tail
        .split(|byte| *byte == b'\n')
        .rev()
        .find(|line| line.iter().any(|byte| !byte.is_ascii_whitespace()))
    else {
        return Ok(0);
    };
    let line = std::str::from_utf8(line)
        .map_err(|_| ReadModelError("final event record is not valid UTF-8".to_string()))?;
    let value: serde_json::Value = serde_json::from_str(line)
        .map_err(|error| ReadModelError(format!("event tail is malformed JSON: {}", error)))?;
    value
        .get("seq")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| ReadModelError("event tail has no unsigned `seq`".to_string()))
}

fn compose_snapshot(
    project: &StrictProject,
    tree: Option<&StrictAcceptedTree>,
    branches: &[StrictBranch],
    narratives: NarrativeSet,
) -> ReadResult<ProjectMapSnapshot> {
    let branch_by_id: HashMap<&str, &StrictBranch> = branches
        .iter()
        .map(|branch| (branch.path.as_str(), branch))
        .collect();
    let (topology_source, topology_nodes): (&str, Vec<StrictAcceptedTreeNode>) =
        if project.tree_revision == 0 {
            let root = branch_by_id.get("root").ok_or_else(|| {
                ReadModelError("revision-zero project has no root branch".to_string())
            })?;
            (
                "bootstrap",
                vec![StrictAcceptedTreeNode {
                    id: "root".to_string(),
                    parent: String::new(),
                    title: root.title.clone(),
                    purpose: root.purpose.clone(),
                    spec: None,
                    sibling_order: 0,
                    depends_on: Vec::new(),
                }],
            )
        } else {
            let accepted = tree.ok_or_else(|| {
                ReadModelError("accepted topology disappeared before projection".to_string())
            })?;
            ("accepted", accepted.nodes.clone())
        };

    let mut child_counts: HashMap<&str, usize> = HashMap::new();
    for node in &topology_nodes {
        if !node.parent.is_empty() {
            *child_counts.entry(node.parent.as_str()).or_default() += 1;
        }
    }
    let mut nodes = Vec::with_capacity(topology_nodes.len());
    for node in &topology_nodes {
        let branch = branch_by_id.get(node.id.as_str()).ok_or_else(|| {
            ReadModelError(format!(
                "accepted topology node `{}` has no lifecycle state",
                node.id
            ))
        })?;
        nodes.push(ProjectMapNode {
            id: node.id.clone(),
            parent: node.parent.clone(),
            order: node.sibling_order,
            title: node.title.clone(),
            purpose: node.purpose.clone(),
            spec: node.spec.clone(),
            status: branch.status.clone(),
            verification: branch.verification_status.clone(),
            status_reason: branch.status_reason.clone(),
            is_current: project.current_branch == node.id,
            readiness: readiness(branch, &node.depends_on, &branch_by_id),
            depends_on: node.depends_on.clone(),
            child_count: child_counts.get(node.id.as_str()).copied().unwrap_or(0),
        });
    }
    let dependencies = topology_nodes
        .iter()
        .flat_map(|node| {
            node.depends_on.iter().map(|dependency| {
                let satisfied = branch_by_id
                    .get(dependency.as_str())
                    .is_some_and(|branch| branch.status == "complete");
                ProjectMapDependency {
                    from: node.id.clone(),
                    to: dependency.clone(),
                    satisfied,
                }
            })
        })
        .collect();
    let narrative_revision = narrative_revision(&narratives.raw_documents);
    Ok(ProjectMapSnapshot {
        projection: ProjectMapProjection {
            schema_version: 1,
            tree_revision: project.tree_revision,
            state_event_seq: project.last_event_seq,
            narrative_revision,
            tree_editing: project.tree_editing.is_some(),
            projected_at: now(),
            health: ProjectMapHealth::ok(),
            project: ProjectMapProject {
                stage: project.stage.clone(),
                current_branch: project.current_branch.clone(),
                topology_source: topology_source.to_string(),
            },
            nodes,
            dependencies,
        },
        narratives: narratives.by_branch,
        managed_watch_roots: narratives.managed_watch_roots,
    })
}

fn readiness(
    branch: &StrictBranch,
    dependencies: &[String],
    branch_by_id: &HashMap<&str, &StrictBranch>,
) -> String {
    match branch.status.as_str() {
        "in_progress" => "active",
        "complete" => "complete",
        "paused" => "paused",
        "aborted" => "aborted",
        "pending"
            if dependencies.iter().all(|dependency| {
                branch_by_id
                    .get(dependency.as_str())
                    .is_some_and(|item| item.status == "complete")
            }) =>
        {
            "ready"
        }
        "pending" => "waiting",
        _ => "waiting",
    }
    .to_string()
}

fn read_narratives(
    root: &Path,
    treework: &Path,
    project: &StrictProject,
    branches: &[StrictBranch],
) -> ReadResult<NarrativeSet> {
    let layout = BranchArtifactLayout::build(
        project.artifact_layout_version,
        branches.iter().map(|branch| BranchArtifactNode {
            id: branch.path.clone(),
            parent: branch.parent.clone(),
        }),
    )
    .map_err(|error| ReadModelError(format!("invalid branch artifact layout: {error}")))?;
    let mut ordered: Vec<&StrictBranch> = branches.iter().collect();
    if project.tree_revision == 0 {
        ordered.retain(|branch| branch.path == "root");
    }
    ordered.sort_by(|left, right| left.path.cmp(&right.path));
    let mut by_branch = HashMap::new();
    let mut raw_documents = Vec::new();
    let mut managed_watch_roots = BTreeSet::new();

    for branch in ordered {
        let source = document_source(root, treework, &layout, branch)?;
        if source.managed {
            managed_watch_roots.insert(source.base.clone());
        }
        let task_plan = read_secure_document(&source, "task_plan.md")?;
        let progress = read_secure_document(&source, "progress.md")?;
        let findings = read_secure_document(&source, "findings.md")?;
        let verification = read_secure_document(&source, "verification.md")?;
        for (name, content) in [
            ("task_plan.md", &task_plan),
            ("progress.md", &progress),
            ("findings.md", &findings),
            ("verification.md", &verification),
        ] {
            raw_documents.push((branch.path.clone(), name.to_string(), content.to_string()));
        }
        by_branch.insert(
            branch.path.clone(),
            parse_narrative(branch, &task_plan, &progress, &findings, &verification),
        );
    }
    Ok(NarrativeSet {
        by_branch,
        raw_documents,
        managed_watch_roots,
    })
}

#[derive(Clone, Debug)]
struct DocumentSource {
    base: PathBuf,
    relative_dir: PathBuf,
    managed: bool,
}

fn document_source(
    root: &Path,
    treework: &Path,
    layout: &BranchArtifactLayout,
    branch: &StrictBranch,
) -> ReadResult<DocumentSource> {
    if branch.path != "root" {
        if let Some(source) = validated_managed_document_source(root, layout, branch) {
            return Ok(source);
        }
    }
    let relative_dir = layout
        .relative_dir(&branch.path)
        .map_err(|error| ReadModelError(error.to_string()))?
        .to_path_buf();
    Ok(DocumentSource {
        base: treework.to_path_buf(),
        relative_dir,
        managed: false,
    })
}

fn validated_managed_document_source(
    root: &Path,
    layout: &BranchArtifactLayout,
    branch: &StrictBranch,
) -> Option<DocumentSource> {
    let isolation = branch.isolation.as_ref()?;
    if isolation.mode != "git-worktree"
        || !isolation.managed_by_treework
        || isolation.workspace_path.trim().is_empty()
    {
        return None;
    }
    let root = fs::canonicalize(root).ok()?;
    let workspace = fs::canonicalize(&isolation.workspace_path).ok()?;
    let git_dir = super::git_path(&workspace, "--git-dir")?;
    let common_dir = fs::canonicalize(super::git_path(&workspace, "--git-common-dir")?).ok()?;
    let control_common_dir = fs::canonicalize(super::git_path(&root, "--git-common-dir")?).ok()?;
    if common_dir != control_common_dir {
        return None;
    }
    let binding: StrictWorktreeBinding =
        read_strict_json(&git_dir.join(WORKTREE_BRANCH_DESCRIPTOR)).ok()?;
    let descriptor: StrictControlDescriptor =
        read_strict_json(&common_dir.join(CONTROL_DESCRIPTOR)).ok()?;
    if binding.version != 1
        || descriptor.version != 1
        || binding.branch != branch.path
        || binding.project_id != descriptor.project_id
        || binding.project_id != super::project_id_for_root(&root)
        || fs::canonicalize(&binding.workspace).ok()? != workspace
        || fs::canonicalize(&descriptor.control_root).ok()? != root
    {
        return None;
    }
    let treework = fs::canonicalize(workspace.join(".TreeWork")).ok()?;
    Some(DocumentSource {
        base: treework,
        relative_dir: layout.relative_dir(&branch.path).ok()?.to_path_buf(),
        managed: true,
    })
}

fn read_secure_document(source: &DocumentSource, name: &str) -> ReadResult<String> {
    let relative = source.relative_dir.join(name);
    let path = secure_join(&source.base, &relative)?;
    if !path.exists() {
        return Ok(String::new());
    }
    let metadata = fs::symlink_metadata(&path).map_err(|error| {
        ReadModelError(format!(
            "cannot inspect Inspector document {}: {}",
            path.display(),
            error
        ))
    })?;
    if metadata.file_type().is_symlink() {
        return Err(ReadModelError(format!(
            "Inspector document {} is a symlink",
            path.display()
        )));
    }
    if !metadata.is_file() {
        return Err(ReadModelError(format!(
            "Inspector document {} is not a regular file",
            path.display()
        )));
    }
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err(ReadModelError(format!(
            "Inspector document {} exceeds the size limit",
            path.display()
        )));
    }
    fs::read_to_string(&path).map_err(|error| {
        ReadModelError(format!(
            "cannot read Inspector document {}: {}",
            path.display(),
            error
        ))
    })
}

fn secure_join(base: &Path, relative: &Path) -> ReadResult<PathBuf> {
    let canonical_base = canonical_directory(base, "Inspector document root")?;
    let mut cursor = canonical_base.clone();
    for component in relative.components() {
        let Component::Normal(segment) = component else {
            return Err(ReadModelError(format!(
                "Inspector path `{}` contains an unsafe component",
                relative.display()
            )));
        };
        cursor.push(segment);
        if let Ok(metadata) = fs::symlink_metadata(&cursor) {
            if metadata.file_type().is_symlink() {
                return Err(ReadModelError(format!(
                    "Inspector path `{}` crosses symlink `{}`",
                    relative.display(),
                    cursor.display()
                )));
            }
        }
    }
    if cursor.exists() {
        let canonical = fs::canonicalize(&cursor).map_err(|error| {
            ReadModelError(format!(
                "cannot resolve Inspector path {}: {}",
                cursor.display(),
                error
            ))
        })?;
        if !canonical.starts_with(&canonical_base) {
            return Err(ReadModelError(format!(
                "Inspector path `{}` escapes its document root",
                relative.display()
            )));
        }
    }
    Ok(cursor)
}

fn parse_narrative(
    branch: &StrictBranch,
    task_plan: &str,
    progress: &str,
    findings: &str,
    verification: &str,
) -> BranchNarrative {
    let task_sections = markdown_sections(task_plan);
    let progress_sections = markdown_sections(progress);
    let finding_sections = markdown_sections(findings);
    let verification_sections = markdown_sections(verification);
    let mut warnings = Vec::new();

    let task_plan = TaskPlanSections {
        scope: section_with_aliases(
            &task_sections,
            &["scope", "project scope"],
            "task_plan.scope",
            &mut warnings,
        ),
        acceptance: section_with_aliases(
            &task_sections,
            &["acceptance", "project acceptance"],
            "task_plan.acceptance",
            &mut warnings,
        ),
        local_steps: section_with_aliases(
            &task_sections,
            &["local steps", "roadmap"],
            "task_plan.local_steps",
            &mut warnings,
        ),
        out_of_scope: section_with_aliases(
            &task_sections,
            &["out of scope"],
            "task_plan.out_of_scope",
            &mut warnings,
        ),
        dependencies: section_with_aliases(
            &task_sections,
            &["dependencies", "external dependencies"],
            "task_plan.dependencies",
            &mut warnings,
        ),
        branch_intake_gate: section_with_aliases(
            &task_sections,
            &["branch intake gate"],
            "task_plan.branch_intake_gate",
            &mut warnings,
        ),
    };
    let progress = ProgressSections {
        current_reality: section_with_aliases(
            &progress_sections,
            &["current reality", "global reality"],
            "progress.current_reality",
            &mut warnings,
        ),
        recent_work: section_with_aliases(
            &progress_sections,
            &["recent work", "recent branch returns"],
            "progress.recent_work",
            &mut warnings,
        ),
        open_issues: section_with_aliases(
            &progress_sections,
            &["open issues", "unverified or paused work"],
            "progress.open_issues",
            &mut warnings,
        ),
        exit_notes: section_with_aliases(
            &progress_sections,
            &["exit notes", "next routing"],
            "progress.exit_notes",
            &mut warnings,
        ),
    };
    let findings = FindingsSections {
        decisions: section_with_aliases(
            &finding_sections,
            &["decisions"],
            "findings.decisions",
            &mut warnings,
        ),
        interface_or_contract_effects: section_with_aliases(
            &finding_sections,
            &["interface or contract effects"],
            "findings.interface_or_contract_effects",
            &mut warnings,
        ),
        risks_and_unknowns: section_with_aliases(
            &finding_sections,
            &["risks and unknowns"],
            "findings.risks_and_unknowns",
            &mut warnings,
        ),
    };
    let evidence = section_with_aliases(
        &verification_sections,
        &["evidence", "latest verification"],
        "verification.evidence",
        &mut warnings,
    );
    let coverage_gap = verification_sections
        .get("coverage gap")
        .cloned()
        .or_else(|| extract_labeled_value(&evidence, "Coverage gap"))
        .unwrap_or_else(|| {
            warnings.push("missing Inspector section `verification.coverage_gap`".to_string());
            String::new()
        });
    let verification = VerificationSections {
        status: branch.verification_status.clone(),
        evidence,
        coverage_gap,
    };
    BranchNarrative {
        task_plan,
        progress,
        findings,
        verification,
        warnings,
    }
}

fn markdown_sections(markdown: &str) -> HashMap<String, String> {
    let lines: Vec<&str> = markdown.lines().collect();
    let mut headings = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(heading) = line.strip_prefix("## ") {
            headings.push((index, normalize_heading(heading)));
        }
    }
    let mut sections = HashMap::new();
    for (position, (line_index, heading)) in headings.iter().enumerate() {
        let end = headings
            .get(position + 1)
            .map(|(index, _)| *index)
            .unwrap_or(lines.len());
        let body = lines[line_index + 1..end].join("\n").trim().to_string();
        sections.entry(heading.clone()).or_insert(body);
    }
    sections
}

fn normalize_heading(heading: &str) -> String {
    heading
        .split_once(" (")
        .map(|(prefix, _)| prefix)
        .unwrap_or(heading)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn section_with_aliases(
    sections: &HashMap<String, String>,
    aliases: &[&str],
    label: &str,
    warnings: &mut Vec<String>,
) -> String {
    for alias in aliases {
        if let Some(value) = sections.get(*alias) {
            return value.clone();
        }
    }
    warnings.push(format!("missing Inspector section `{}`", label));
    String::new()
}

fn extract_labeled_value(markdown: &str, label: &str) -> Option<String> {
    let prefix = format!("- {}:", label).to_ascii_lowercase();
    markdown.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.to_ascii_lowercase().starts_with(&prefix) {
            trimmed
                .split_once(':')
                .map(|(_, value)| value.trim().to_string())
        } else {
            None
        }
    })
}

fn branch_detail_health(global: &ProjectMapHealth, warnings: &[String]) -> ProjectMapHealth {
    if warnings.is_empty() {
        return global.clone();
    }
    let warning_message = warnings.join("; ");
    if global.status == "ok" {
        ProjectMapHealth::warning(warning_message)
    } else {
        let message = if global.message.is_empty() {
            warning_message
        } else {
            format!("{}; {}", global.message, warning_message)
        };
        ProjectMapHealth {
            status: global.status.clone(),
            message,
        }
    }
}

fn empty_narrative() -> BranchNarrative {
    BranchNarrative {
        task_plan: TaskPlanSections::default(),
        progress: ProgressSections::default(),
        findings: FindingsSections::default(),
        verification: VerificationSections::default(),
        warnings: vec!["branch narrative is unavailable".to_string()],
    }
}

fn narrative_revision(documents: &[(String, String, String)]) -> String {
    let mut hasher = Sha256::new();
    for (branch, name, content) in documents {
        for value in [branch, name, content] {
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value.as_bytes());
        }
    }
    format!("sha256:{}", hex_bytes(hasher.finalize().as_ref()))
}

fn empty_narrative_revision() -> String {
    format!("sha256:{}", hex_bytes(Sha256::digest([]).as_ref()))
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn validate_branch_id(id: &str) -> ReadResult<()> {
    if id.is_empty() || id.len() > 160 || id.contains('\\') {
        return Err(ReadModelError(format!("invalid branch ID `{}`", id)));
    }
    let path = Path::new(id);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || id
            .split('/')
            .any(|segment| segment.is_empty() || matches!(segment, "." | ".."))
    {
        return Err(ReadModelError(format!("invalid branch ID `{}`", id)));
    }
    let mut chars = id.chars();
    if !chars
        .next()
        .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
        || !id.chars().all(|value| {
            value.is_ascii_lowercase()
                || value.is_ascii_digit()
                || matches!(value, '.' | '_' | '/' | '-')
        })
    {
        return Err(ReadModelError(format!("invalid branch ID `{}`", id)));
    }
    Ok(())
}

fn validate_spec_path(spec: &str) -> ReadResult<()> {
    let path = Path::new(spec);
    if spec.trim().is_empty()
        || spec.len() > 500
        || spec.contains('\\')
        || !spec.ends_with(".md")
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ReadModelError(format!(
            "invalid accepted Spec path `{}`",
            spec
        )));
    }
    Ok(())
}

fn ensure_lock_absent(root: &Path) -> ReadResult<()> {
    let lock = root.join(".TreeWork.lock");
    match fs::symlink_metadata(&lock) {
        Ok(_) => Err(ReadModelError(format!(
            "TreeWork transaction lock is present at {}",
            lock.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ReadModelError(format!(
            "cannot inspect TreeWork transaction lock {}: {}",
            lock.display(),
            error
        ))),
    }
}

fn canonical_directory(path: &Path, label: &str) -> ReadResult<PathBuf> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        ReadModelError(format!(
            "cannot resolve {} {}: {}",
            label,
            path.display(),
            error
        ))
    })?;
    if !canonical.is_dir() {
        return Err(ReadModelError(format!(
            "{} {} is not a directory",
            label,
            path.display()
        )));
    }
    Ok(canonical)
}

fn read_strict_json<T: for<'de> Deserialize<'de>>(path: &Path) -> ReadResult<T> {
    let metadata = fs::metadata(path).map_err(|error| {
        ReadModelError(format!("cannot inspect JSON {}: {}", path.display(), error))
    })?;
    if !metadata.is_file() || metadata.len() > MAX_STATE_BYTES {
        return Err(ReadModelError(format!(
            "JSON {} is not a regular bounded file",
            path.display()
        )));
    }
    let source = fs::read_to_string(path).map_err(|error| {
        ReadModelError(format!("cannot read JSON {}: {}", path.display(), error))
    })?;
    serde_json::from_str(&source)
        .map_err(|error| ReadModelError(format!("malformed JSON {}: {}", path.display(), error)))
}

fn stable_hash_str(value: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{:016x}", hash)
}

fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", seconds)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use serde_json::Value;
    use tempfile::TempDir;

    pub(crate) struct TestFixture {
        _temp: TempDir,
        pub(crate) root: PathBuf,
    }

    impl TestFixture {
        pub(crate) fn accepted() -> Self {
            let temp = tempfile::tempdir().expect("temporary project");
            let root = temp.path().join("control");
            fs::create_dir(&root).expect("control project");
            create_layout(&root);
            write_documents(&root, "initial feature reality");
            let fixture = Self { _temp: temp, root };
            fixture.publish(1, 1, "Fixture Root", "complete", "pending");
            fixture
        }

        pub(crate) fn revision_zero() -> Self {
            let temp = tempfile::tempdir().expect("temporary project");
            let root = temp.path().join("control");
            fs::create_dir(&root).expect("control project");
            create_layout(&root);
            write_documents(&root, "bootstrap reality");
            write_json(
                &root.join(".TreeWork/state/project.json"),
                &json!({
                    "schema_version": "0.1",
                    "stage": "alignment",
                    "current_branch": "root",
                    "last_event_seq": 1,
                    "tree_revision": 0,
                    "tree_editing": null,
                    "tree_hash": "",
                    "last_sync": "unix:1"
                }),
            );
            write_json(
                &root.join(".TreeWork/state/branches.json"),
                &json!({
                    "branches": [
                        branch_json("root", "", "Bootstrap Root", "in_progress", "unverified")
                    ]
                }),
            );
            write_json(
                &root.join(".TreeWork/state/graph.json"),
                &json!({"edges": []}),
            );
            fs::write(root.join(".TreeWork/events.jsonl"), "{\"seq\":1}\n").expect("event log");
            Self { _temp: temp, root }
        }

        pub(crate) fn store(&self) -> ProjectMapStore {
            ProjectMapStore::with_stability_delay(self.root.clone(), Duration::ZERO)
        }

        pub(crate) fn publish(
            &self,
            revision: u64,
            event_seq: u64,
            root_title: &str,
            foundation_status: &str,
            feature_status: &str,
        ) {
            let source_hash = format!("fnv1a64:source-{}", revision);
            let mut tree = StrictAcceptedTree {
                schema_version: 1,
                revision,
                source_hash: source_hash.clone(),
                state_hash: String::new(),
                accepted_at: format!("unix:{}", event_seq),
                root: "root".to_string(),
                nodes: vec![
                    StrictAcceptedTreeNode {
                        id: "root".to_string(),
                        parent: String::new(),
                        title: root_title.to_string(),
                        purpose: "Coordinate the fixture.".to_string(),
                        spec: Some("spec.md".to_string()),
                        sibling_order: 0,
                        depends_on: Vec::new(),
                    },
                    StrictAcceptedTreeNode {
                        id: "foundation".to_string(),
                        parent: "root".to_string(),
                        title: "Foundation".to_string(),
                        purpose: "Provide the prerequisite.".to_string(),
                        spec: None,
                        sibling_order: 0,
                        depends_on: Vec::new(),
                    },
                    StrictAcceptedTreeNode {
                        id: "feature".to_string(),
                        parent: "root".to_string(),
                        title: "Feature".to_string(),
                        purpose: "Exercise the read model.".to_string(),
                        spec: Some("branches/feature/spec.md".to_string()),
                        sibling_order: 1,
                        depends_on: vec!["foundation".to_string()],
                    },
                    StrictAcceptedTreeNode {
                        id: "waiting".to_string(),
                        parent: "root".to_string(),
                        title: "Waiting".to_string(),
                        purpose: "Exercise blocked readiness.".to_string(),
                        spec: None,
                        sibling_order: 2,
                        depends_on: vec!["feature".to_string()],
                    },
                ],
            };
            tree.state_hash = strict_tree_state_hash(&tree).expect("tree hash");
            write_json(
                &self.root.join(".TreeWork/state/tree.json"),
                &serde_json::to_value(&tree).expect("tree JSON"),
            );
            write_json(
                &self.root.join(".TreeWork/state/branches.json"),
                &json!({
                    "branches": [
                        branch_json("root", "", root_title, "in_progress", "unverified"),
                        branch_json(
                            "foundation",
                            "root",
                            "Foundation",
                            foundation_status,
                            if foundation_status == "complete" { "verified" } else { "partial" }
                        ),
                        branch_json(
                            "feature",
                            "root",
                            "Feature",
                            feature_status,
                            "unverified"
                        ),
                        branch_json(
                            "waiting",
                            "root",
                            "Waiting",
                            "pending",
                            "unverified"
                        )
                    ]
                }),
            );
            write_json(
                &self.root.join(".TreeWork/state/graph.json"),
                &json!({
                    "edges": [
                        edge_json("edge-1", "root", "foundation", "parent_of"),
                        edge_json("edge-2", "root", "feature", "parent_of"),
                        edge_json("edge-3", "root", "waiting", "parent_of"),
                        edge_json("edge-4", "feature", "foundation", "depends_on"),
                        edge_json("edge-5", "waiting", "feature", "depends_on")
                    ]
                }),
            );
            fs::write(
                self.root.join(".TreeWork/events.jsonl"),
                format!("{{\"seq\":{}}}\n", event_seq),
            )
            .expect("event log");
            write_json(
                &self.root.join(".TreeWork/state/project.json"),
                &json!({
                    "schema_version": "0.1",
                    "stage": "work_tree",
                    "current_branch": "feature",
                    "last_event_seq": event_seq,
                    "tree_revision": revision,
                    "tree_editing": null,
                    "tree_hash": source_hash,
                    "last_sync": format!("unix:{}", event_seq)
                }),
            );
        }

        pub(crate) fn write_feature_progress(&self, reality: &str) {
            fs::write(
                self.root.join(".TreeWork/branches/feature/progress.md"),
                progress_document(reality),
            )
            .expect("feature progress");
        }
    }

    fn create_layout(root: &Path) {
        for path in [
            ".TreeWork/state",
            ".TreeWork/branches/foundation",
            ".TreeWork/branches/feature",
            ".TreeWork/branches/waiting",
            ".TreeWork/out",
        ] {
            fs::create_dir_all(root.join(path)).expect("fixture directory");
        }
    }

    fn write_documents(root: &Path, feature_reality: &str) {
        for branch in ["root", "foundation", "feature", "waiting"] {
            let directory = if branch == "root" {
                root.join(".TreeWork")
            } else {
                root.join(".TreeWork/branches").join(branch)
            };
            fs::write(directory.join("task_plan.md"), task_plan_document()).expect("task plan");
            fs::write(
                directory.join("progress.md"),
                progress_document(if branch == "feature" {
                    feature_reality
                } else {
                    "fixture reality"
                }),
            )
            .expect("progress");
            fs::write(directory.join("findings.md"), findings_document()).expect("findings");
            fs::write(directory.join("verification.md"), verification_document())
                .expect("verification");
        }
    }

    fn task_plan_document() -> &'static str {
        "# Task Plan\n\n## Scope (owned work)\n\nScope body.\n\n## Acceptance\n\n- [ ] Acceptance body.\n\n## Local Steps\n\n- [ ] Local body.\n\n## Out Of Scope\n\nOut body.\n\n## Dependencies\n\n1. Dependency body.\n\n## Branch Intake Gate\n\nIntake body.\n"
    }

    fn progress_document(reality: &str) -> String {
        format!(
            "# Progress\n\n## Current Reality\n\n{}\n\n## Recent Work\n\nRecent body.\n\n## Open Issues\n\nOpen body.\n\n## Exit Notes\n\nExit body.\n",
            reality
        )
    }

    fn findings_document() -> &'static str {
        "# Findings\n\n## Decisions\n\nDecision body.\n\n## Interface Or Contract Effects\n\nContract body.\n\n## Risks And Unknowns\n\nRisk body.\n"
    }

    fn verification_document() -> &'static str {
        "# Verification\n\n## Evidence\n\n- Command: `cargo test`\n- Result: passed\n\n## Coverage Gap\n\nNone.\n"
    }

    fn branch_json(
        path: &str,
        parent: &str,
        title: &str,
        status: &str,
        verification: &str,
    ) -> Value {
        json!({
            "path": path,
            "parent": parent,
            "title": title,
            "purpose": format!("Purpose for {}.", path),
            "status": status,
            "verification_status": verification,
            "sync_status": "clean",
            "last_sync": "unix:1"
        })
    }

    fn edge_json(id: &str, from: &str, to: &str, kind: &str) -> Value {
        json!({
            "id": id,
            "from": from,
            "to": to,
            "kind": kind,
            "user_label": format!("{} {} {}", from, kind, to),
            "interpreted_relation": kind
        })
    }

    pub(crate) fn write_json(path: &Path, value: &Value) {
        fs::write(
            path,
            format!(
                "{}\n",
                serde_json::to_string_pretty(value).expect("serialize fixture")
            ),
        )
        .expect("write fixture JSON");
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::{write_json, TestFixture};
    use super::*;
    use serde_json::Value;
    use std::process::Command;

    #[test]
    fn coherent_projection_derives_readiness_and_sections() {
        let fixture = TestFixture::accepted();
        let store = fixture.store();
        let _ = store.refresh();
        let projection = store.projection().expect("projection");
        let feature = projection
            .nodes
            .iter()
            .find(|node| node.id == "feature")
            .expect("feature node");
        let waiting = projection
            .nodes
            .iter()
            .find(|node| node.id == "waiting")
            .expect("waiting node");
        assert_eq!(feature.readiness, "ready");
        assert_eq!(waiting.readiness, "waiting");
        assert_eq!(projection.dependencies.len(), 2);
        assert!(projection.dependencies[0].satisfied);
        assert!(projection.narrative_revision.starts_with("sha256:"));

        let detail = store.branch_detail("feature").expect("branch detail");
        assert_eq!(detail.health.status, "ok");
        assert_eq!(detail.task_plan.scope, "Scope body.");
        assert_eq!(detail.progress.current_reality, "initial feature reality");
        assert_eq!(
            detail.findings.interface_or_contract_effects,
            "Contract body."
        );
        assert_eq!(detail.verification.coverage_gap, "None.");
    }

    #[test]
    fn hierarchical_layout_reads_nested_branch_narratives() {
        let fixture = TestFixture::accepted();
        let project_path = fixture.root.join(".TreeWork/state/project.json");
        let mut project: Value =
            serde_json::from_str(&fs::read_to_string(&project_path).expect("project source"))
                .expect("project JSON");
        project["artifact_layout_version"] = json!(HIERARCHICAL_LAYOUT);
        write_json(&project_path, &project);

        let branches_path = fixture.root.join(".TreeWork/state/branches.json");
        let mut branches: Value =
            serde_json::from_str(&fs::read_to_string(&branches_path).expect("branches source"))
                .expect("branches JSON");
        let feature = branches["branches"]
            .as_array_mut()
            .expect("branch array")
            .iter_mut()
            .find(|branch| branch["path"] == "feature")
            .expect("feature branch");
        feature["parent"] = json!("foundation");
        write_json(&branches_path, &branches);

        let tree_path = fixture.root.join(".TreeWork/state/tree.json");
        let mut tree: StrictAcceptedTree = read_strict_json(&tree_path).expect("accepted tree");
        let feature = tree
            .nodes
            .iter_mut()
            .find(|node| node.id == "feature")
            .expect("feature node");
        feature.parent = "foundation".to_string();
        feature.spec = Some("branches/foundation/feature/spec.md".to_string());
        tree.state_hash = strict_tree_state_hash(&tree).expect("tree hash");
        write_json(
            &tree_path,
            &serde_json::to_value(&tree).expect("tree value"),
        );

        let graph_path = fixture.root.join(".TreeWork/state/graph.json");
        let mut graph: Value =
            serde_json::from_str(&fs::read_to_string(&graph_path).expect("graph source"))
                .expect("graph JSON");
        let edge = graph["edges"]
            .as_array_mut()
            .expect("edge array")
            .iter_mut()
            .find(|edge| edge["kind"] == "parent_of" && edge["to"] == "feature")
            .expect("feature parent edge");
        edge["from"] = json!("foundation");
        write_json(&graph_path, &graph);

        let old = fixture.root.join(".TreeWork/branches/feature");
        let nested = fixture.root.join(".TreeWork/branches/foundation/feature");
        fs::rename(&old, &nested).expect("nest feature documents");

        let store = fixture.store();
        let _ = store.refresh();
        assert_eq!(
            store
                .projection()
                .expect("hierarchical projection")
                .health
                .status,
            "ok"
        );
        assert_eq!(
            store
                .branch_detail("feature")
                .expect("feature detail")
                .progress
                .current_reality,
            "initial feature reality"
        );
    }

    #[test]
    fn revision_zero_projects_only_bootstrap_root() {
        let fixture = TestFixture::revision_zero();
        let store = fixture.store();
        let _ = store.refresh();
        let projection = store.projection().expect("bootstrap projection");
        assert_eq!(projection.tree_revision, 0);
        assert_eq!(projection.project.topology_source, "bootstrap");
        assert_eq!(projection.nodes.len(), 1);
        assert_eq!(projection.nodes[0].id, "root");
        assert_eq!(projection.nodes[0].child_count, 0);
        assert!(projection.dependencies.is_empty());
    }

    #[test]
    fn lock_prevents_a_coherent_read() {
        let fixture = TestFixture::accepted();
        fs::create_dir(fixture.root.join(".TreeWork.lock")).expect("lock directory");
        let error = load_coherent_snapshot(&fixture.root, Duration::ZERO)
            .expect_err("lock must block read");
        assert!(error.to_string().contains("transaction lock"));
    }

    #[test]
    fn marker_race_is_rejected() {
        let fixture = TestFixture::accepted();
        let project_path = fixture.root.join(".TreeWork/state/project.json");
        let error = load_coherent_snapshot_with_hook(&fixture.root, Duration::ZERO, || {
            let mut project: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(&project_path).expect("project"))
                    .expect("project JSON");
            project["last_sync"] = json!("unix:changed");
            write_json(&project_path, &project);
        })
        .expect_err("marker race");
        assert!(error.to_string().contains("marker changed"));
    }

    #[test]
    fn hash_mismatch_retains_last_good_and_degrades_health() {
        let fixture = TestFixture::accepted();
        let store = fixture.store();
        let _ = store.refresh();
        let tree_path = fixture.root.join(".TreeWork/state/tree.json");
        let mut tree: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&tree_path).expect("tree"))
                .expect("tree JSON");
        tree["state_hash"] = json!("fnv1a64:bad");
        write_json(&tree_path, &tree);

        let report = store.refresh();
        let projection = store.projection().expect("last good");
        assert_eq!(projection.health.status, "degraded");
        assert!(projection.health.message.contains("hash"));
        assert_eq!(projection.tree_revision, 1);
        assert_eq!(
            report.invalidation.expect("health invalidation").changes,
            vec!["health"]
        );

        fixture.publish(1, 1, "Fixture Root", "complete", "pending");
        let report = store.refresh();
        assert_eq!(
            store
                .projection()
                .expect("recovered projection")
                .health
                .status,
            "ok"
        );
        assert_eq!(
            report.invalidation.expect("recovery invalidation").changes,
            vec!["health"]
        );
    }

    #[test]
    fn partial_jsonl_retains_last_good_projection() {
        let fixture = TestFixture::accepted();
        let store = fixture.store();
        let _ = store.refresh();
        fs::write(
            fixture.root.join(".TreeWork/events.jsonl"),
            "{\"seq\":1}\n{\"seq\":2",
        )
        .expect("partial event");
        let _ = store.refresh();
        let projection = store.projection().expect("last good");
        assert_eq!(projection.state_event_seq, 1);
        assert_eq!(projection.health.status, "degraded");
        assert!(projection.health.message.contains("partial JSONL"));
    }

    #[test]
    fn event_tail_ignores_opaque_bytes_before_the_final_complete_record() {
        let directory = tempfile::tempdir().expect("event tail directory");
        let path = directory.path().join("events.jsonl");
        let mut bytes = vec![0xff; MAX_EVENT_TAIL_BYTES as usize + 32];
        bytes.extend_from_slice(b"\n{\"seq\":42}\n");
        fs::write(&path, bytes).expect("event tail");
        assert_eq!(read_event_tail_seq(&path).expect("final event seq"), 42);
    }

    #[test]
    fn strict_models_reject_missing_required_state() {
        let fixture = TestFixture::accepted();
        let path = fixture.root.join(".TreeWork/state/branches.json");
        let mut state: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).expect("branches"))
                .expect("branch JSON");
        state["branches"][1]
            .as_object_mut()
            .expect("branch object")
            .remove("status");
        write_json(&path, &state);
        let error =
            load_coherent_snapshot(&fixture.root, Duration::ZERO).expect_err("missing status");
        assert!(error.to_string().contains("missing field `status`"));
    }

    #[test]
    fn narrative_requires_two_identical_reads() {
        let fixture = TestFixture::accepted();
        let progress = fixture.root.join(".TreeWork/branches/feature/progress.md");
        let error = load_coherent_snapshot_with_hooks(
            &fixture.root,
            Duration::ZERO,
            || {},
            || {
                fs::write(
                    &progress,
                    "# Progress\n\n## Current Reality\n\nchanged between reads\n",
                )
                .expect("change narrative");
            },
        )
        .expect_err("unstable narrative");
        assert!(error.to_string().contains("stable narrative"));
    }

    #[test]
    fn one_refresh_coalesces_sse_change_categories() {
        let fixture = TestFixture::accepted();
        let store = fixture.store();
        let _ = store.refresh();
        fixture.write_feature_progress("updated narrative");
        fixture.publish(2, 2, "Updated Root", "paused", "pending");
        let report = store.refresh();
        assert_eq!(
            report.invalidation.expect("invalidation").changes,
            vec!["topology", "state", "narrative", "events"]
        );
    }

    #[test]
    fn opaque_event_only_commit_emits_only_events_category() {
        let fixture = TestFixture::accepted();
        let store = fixture.store();
        let _ = store.refresh();
        let project_path = fixture.root.join(".TreeWork/state/project.json");
        let mut project: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&project_path).expect("project"))
                .expect("project JSON");
        project["last_event_seq"] = json!(2);
        project["last_sync"] = json!("unix:2");
        fs::write(fixture.root.join(".TreeWork/events.jsonl"), "{\"seq\":2}\n").expect("event");
        write_json(&project_path, &project);
        let report = store.refresh();
        assert_eq!(
            report.invalidation.expect("event invalidation").changes,
            vec!["events"]
        );
    }

    #[test]
    fn branch_ids_reject_traversal_and_absolute_paths() {
        for invalid in ["../escape", "alpha/../escape", "/absolute", "alpha\\escape"] {
            assert!(validate_branch_id(invalid).is_err(), "{invalid}");
        }
        assert!(validate_branch_id("alpha/child").is_ok());
    }

    #[test]
    fn accepted_spec_paths_reject_unsafe_components() {
        for invalid in [
            "",
            "../spec.md",
            "branches/../spec.md",
            "/absolute/spec.md",
            "branches\\spec.md",
            "branches/spec.txt",
        ] {
            assert!(validate_spec_path(invalid).is_err(), "{invalid}");
        }
        assert!(validate_spec_path("branches/alpha/spec.md").is_ok());
    }

    #[test]
    fn inspector_prefers_only_a_canonically_bound_managed_worktree() {
        let fixture = TestFixture::accepted();
        run_git(&fixture.root, &["init"]);
        run_git(&fixture.root, &["config", "user.name", "TreeWork Test"]);
        run_git(
            &fixture.root,
            &["config", "user.email", "treework@example.invalid"],
        );
        run_git(&fixture.root, &["add", "."]);
        run_git(&fixture.root, &["commit", "-m", "fixture"]);

        let managed = fixture
            .root
            .parent()
            .expect("fixture parent")
            .join("managed");
        run_git(
            &fixture.root,
            &[
                "worktree",
                "add",
                "-b",
                "managed-feature",
                managed.to_str().expect("managed path"),
            ],
        );
        let canonical_root = fs::canonicalize(&fixture.root).expect("control root");
        let canonical_managed = fs::canonicalize(&managed).expect("managed root");
        let project_id = super::super::project_id_for_root(&canonical_root);
        let common_dir =
            super::super::git_path(&fixture.root, "--git-common-dir").expect("common git dir");
        let git_dir = super::super::git_path(&managed, "--git-dir").expect("worktree git dir");
        fs::create_dir_all(common_dir.join("treework")).expect("descriptor directory");
        write_json(
            &common_dir.join(CONTROL_DESCRIPTOR),
            &json!({
                "version": 1,
                "project_id": project_id,
                "control_root": canonical_root
            }),
        );
        write_json(
            &git_dir.join(WORKTREE_BRANCH_DESCRIPTOR),
            &json!({
                "version": 1,
                "project_id": project_id,
                "branch": "feature",
                "workspace": canonical_managed
            }),
        );
        fs::write(
            managed.join(".TreeWork/branches/feature/progress.md"),
            "# Progress\n\n## Current Reality\n\nmanaged reality\n\n## Recent Work\n\nmanaged\n\n## Open Issues\n\nnone\n\n## Exit Notes\n\nnone\n",
        )
        .expect("managed progress");

        let branches_path = fixture.root.join(".TreeWork/state/branches.json");
        let mut branches: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&branches_path).expect("branches"))
                .expect("branch JSON");
        let feature = branches["branches"]
            .as_array_mut()
            .expect("branch array")
            .iter_mut()
            .find(|branch| branch["path"] == "feature")
            .expect("feature branch");
        feature["isolation"] = json!({
            "mode": "git-worktree",
            "workspace_path": canonical_managed,
            "git_branch": "managed-feature",
            "managed_by_treework": true,
            "created_at": "unix:1",
            "last_entered_at": "unix:1",
            "last_status": "created managed worktree"
        });
        write_json(&branches_path, &branches);

        let store = fixture.store();
        let _ = store.refresh();
        assert_eq!(
            store
                .branch_detail("feature")
                .expect("managed detail")
                .progress
                .current_reality,
            "managed reality"
        );
        assert!(store
            .managed_watch_roots()
            .contains(&canonical_managed.join(".TreeWork")));

        let mut binding: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(git_dir.join(WORKTREE_BRANCH_DESCRIPTOR)).expect("binding"),
        )
        .expect("binding JSON");
        binding["branch"] = json!("wrong-branch");
        write_json(&git_dir.join(WORKTREE_BRANCH_DESCRIPTOR), &binding);
        let _ = store.refresh();
        assert_eq!(
            store
                .branch_detail("feature")
                .expect("fallback detail")
                .progress
                .current_reality,
            "initial feature reality"
        );

        let unrelated = fixture
            .root
            .parent()
            .expect("fixture parent")
            .join("unrelated");
        fs::create_dir(&unrelated).expect("unrelated repository");
        run_git(&unrelated, &["init"]);
        fs::create_dir_all(unrelated.join(".TreeWork/branches/feature"))
            .expect("unrelated narrative");
        fs::write(
            unrelated.join(".TreeWork/branches/feature/progress.md"),
            "# Progress\n\n## Current Reality\n\nunrelated reality\n",
        )
        .expect("unrelated progress");
        let canonical_unrelated = fs::canonicalize(&unrelated).expect("unrelated root");
        let unrelated_common =
            super::super::git_path(&unrelated, "--git-common-dir").expect("unrelated common dir");
        let unrelated_git =
            super::super::git_path(&unrelated, "--git-dir").expect("unrelated git dir");
        fs::create_dir_all(unrelated_common.join("treework")).expect("unrelated descriptors");
        write_json(
            &unrelated_common.join(CONTROL_DESCRIPTOR),
            &json!({
                "version": 1,
                "project_id": project_id,
                "control_root": canonical_root
            }),
        );
        write_json(
            &unrelated_git.join(WORKTREE_BRANCH_DESCRIPTOR),
            &json!({
                "version": 1,
                "project_id": project_id,
                "branch": "feature",
                "workspace": canonical_unrelated
            }),
        );
        let feature = branches["branches"]
            .as_array_mut()
            .expect("branch array")
            .iter_mut()
            .find(|branch| branch["path"] == "feature")
            .expect("feature branch");
        feature["isolation"]["workspace_path"] = json!(canonical_unrelated);
        write_json(&branches_path, &branches);
        let _ = store.refresh();
        assert_eq!(
            store
                .branch_detail("feature")
                .expect("unrelated fallback detail")
                .progress
                .current_reality,
            "initial feature reality"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_inspector_document_degrades_without_replacing_last_good() {
        use std::os::unix::fs::symlink;

        let fixture = TestFixture::accepted();
        let store = fixture.store();
        let _ = store.refresh();
        let progress = fixture.root.join(".TreeWork/branches/feature/progress.md");
        let outside = fixture.root.join("outside.md");
        fs::write(&outside, "# Progress\n\noutside\n").expect("outside document");
        fs::remove_file(&progress).expect("remove progress");
        symlink(&outside, &progress).expect("symlink progress");
        let _ = store.refresh();
        let detail = store.branch_detail("feature").expect("last-good detail");
        assert_eq!(detail.health.status, "degraded");
        assert!(detail.health.message.contains("symlink"));
        assert_eq!(detail.progress.current_reality, "initial feature reality");
    }

    #[test]
    fn markdown_parser_does_not_guess_missing_sections() {
        let branch = StrictBranch {
            path: "alpha".to_string(),
            parent: "root".to_string(),
            title: "Alpha".to_string(),
            purpose: "Test parsing.".to_string(),
            scope: None,
            intake_rationale: String::new(),
            status: "pending".to_string(),
            verification_status: "unverified".to_string(),
            sync_status: "clean".to_string(),
            isolation: None,
            status_reason: String::new(),
            last_sync: Some("unix:1".to_string()),
        };
        let parsed = parse_narrative(&branch, "# Task Plan\n\nUnrelated prose.", "", "", "");
        assert!(parsed.task_plan.scope.is_empty());
        assert!(parsed
            .warnings
            .iter()
            .any(|warning| warning.contains("task_plan.scope")));
    }

    fn run_git(root: &Path, arguments: &[&str]) {
        let output = Command::new("git")
            .args(arguments)
            .current_dir(root)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
