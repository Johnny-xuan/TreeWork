use crate::checkpoint::{load_checkpoint, CheckpointBranch, CheckpointProject, TreeCheckpoint};
use crate::event::{EventData, EventEnvelope, ParsedEvent};
use crate::project_map_read_model::{
    ProjectMapDependency, ProjectMapNode, ProjectMapProject, ProjectMapProjection,
};
use crate::tree_document::AcceptedTreeState;
use crate::TreeEditingSession;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_REPLAY_EVENT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct ReplayRequest {
    pub at: Option<u64>,
    pub after: Option<u64>,
    pub branch: Option<String>,
}

#[derive(Clone, Debug)]
pub(crate) enum ReplayQueryError {
    BadRequest(String),
    NotFound(String),
    Unavailable(String),
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ReplayResponse {
    pub schema_version: u32,
    pub meta: ReplayMeta,
    pub reconstruction: ReplayReconstruction,
    pub state: Option<ReplayState>,
    pub transactions: Vec<ReplayTransaction>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ReplayMeta {
    pub live_event_seq: u64,
    pub at_event_seq: u64,
    pub checkpoint_event_seq: Option<u64>,
    pub earliest_replayable_seq: Option<u64>,
    pub tree_revision: u64,
    pub projected_at: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ReplayReconstruction {
    pub status: String,
    pub gaps: Vec<ReplayGap>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ReplayGap {
    pub from_seq: u64,
    pub to_seq: u64,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ReplayState {
    pub tree_editing: bool,
    pub project: ProjectMapProject,
    pub nodes: Vec<ProjectMapNode>,
    pub dependencies: Vec<ProjectMapDependency>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ReplayTransaction {
    pub seq: u64,
    pub time: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub subject: String,
    pub message: String,
    pub tree_revision: Option<u64>,
    pub affected_subjects: Vec<String>,
    pub changes: Value,
    pub replayable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replayability_reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ReplayMarker {
    schema_version: String,
    stage: String,
    current_branch: String,
    last_event_seq: u64,
    tree_revision: u64,
    tree_editing: Option<TreeEditingSession>,
    tree_hash: String,
    last_sync: String,
}

#[derive(Clone, Debug)]
struct ReplayInputs {
    marker: ReplayMarker,
    events: Vec<ParsedEvent>,
}

#[derive(Clone, Debug)]
struct CheckpointValidation {
    checkpoint: Option<TreeCheckpoint>,
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct WorkingState {
    tree_revision: u64,
    project: CheckpointProject,
    tree: Option<AcceptedTreeState>,
    branches: Vec<CheckpointBranch>,
}

impl WorkingState {
    fn from_checkpoint(checkpoint: &TreeCheckpoint) -> Self {
        Self {
            tree_revision: checkpoint.tree_revision,
            project: checkpoint.project.clone(),
            tree: checkpoint.tree.clone(),
            branches: checkpoint.branches.clone(),
        }
    }

    fn branch_mut(&mut self, id: &str) -> Result<&mut CheckpointBranch, String> {
        self.branches
            .iter_mut()
            .find(|branch| branch.id == id)
            .ok_or_else(|| {
                format!(
                    "event references branch `{}` absent from checkpoint state",
                    id
                )
            })
    }
}

pub(crate) fn project_replay(
    root: &Path,
    live: &ProjectMapProjection,
    request: ReplayRequest,
) -> Result<ReplayResponse, ReplayQueryError> {
    let inputs = read_replay_inputs(root).map_err(ReplayQueryError::Unavailable)?;
    if live.state_event_seq != inputs.marker.last_event_seq
        || live.tree_revision != inputs.marker.tree_revision
    {
        return Err(ReplayQueryError::Unavailable(
            "current Project Map projection has not converged with the publication marker"
                .to_string(),
        ));
    }

    let at = request.at.unwrap_or(inputs.marker.last_event_seq);
    if at == 0 || at > inputs.marker.last_event_seq {
        return Err(ReplayQueryError::BadRequest(format!(
            "`at` must be between 1 and {}",
            inputs.marker.last_event_seq
        )));
    }
    let after = request.after.unwrap_or(0);
    if after > at {
        return Err(ReplayQueryError::BadRequest(format!(
            "`after` sequence {} cannot be greater than `at` sequence {}",
            after, at
        )));
    }
    let validations = validate_checkpoints(root, &inputs.events);
    if let Some(branch) = request.branch.as_deref() {
        if branch.trim().is_empty() {
            return Err(ReplayQueryError::BadRequest(
                "`branch` must not be empty".to_string(),
            ));
        }
        if !known_replay_branch(branch, live, &inputs.events, &validations) {
            return Err(ReplayQueryError::NotFound(branch.to_string()));
        }
    }

    let earliest_replayable_seq = validations
        .iter()
        .filter_map(|(seq, validation)| validation.checkpoint.as_ref().map(|_| *seq))
        .min();
    let selected = validations
        .iter()
        .filter_map(|(seq, validation)| {
            if *seq <= at {
                validation
                    .checkpoint
                    .as_ref()
                    .map(|checkpoint| (*seq, checkpoint.clone()))
            } else {
                None
            }
        })
        .max_by_key(|(seq, _)| *seq);

    let mut gaps = Vec::new();
    let mut reduction_failures = HashMap::new();
    let (checkpoint_event_seq, mut state) = match selected {
        Some((seq, checkpoint)) => (Some(seq), Some(WorkingState::from_checkpoint(&checkpoint))),
        None => {
            push_gap(
                &mut gaps,
                1,
                at,
                "no valid checkpoint exists at or before the requested sequence".to_string(),
            );
            (None, None)
        }
    };

    if let Some(checkpoint_seq) = checkpoint_event_seq {
        for event in inputs
            .events
            .iter()
            .filter(|event| event.seq() > checkpoint_seq && event.seq() <= at)
        {
            let seq = event.seq();
            match event {
                ParsedEvent::Current(envelope) => {
                    if matches!(envelope.data, EventData::TreeApplied(_)) {
                        match validations.get(&seq) {
                            Some(CheckpointValidation {
                                checkpoint: Some(checkpoint),
                                ..
                            }) => {
                                state = Some(WorkingState::from_checkpoint(checkpoint));
                            }
                            Some(validation) => {
                                let reason = validation.error.clone().unwrap_or_else(|| {
                                    "tree.apply has no usable checkpoint".to_string()
                                });
                                reduction_failures.insert(seq, reason.clone());
                                push_gap(&mut gaps, seq, seq, reason);
                                state = None;
                            }
                            None => {
                                let reason =
                                    "tree.apply has no checkpoint validation record".to_string();
                                reduction_failures.insert(seq, reason.clone());
                                push_gap(&mut gaps, seq, seq, reason);
                                state = None;
                            }
                        }
                        continue;
                    }

                    let Some(working) = state.as_mut() else {
                        let reason = "state remains unavailable after an unreplayable transaction"
                            .to_string();
                        reduction_failures.insert(seq, reason.clone());
                        push_gap(&mut gaps, seq, seq, reason);
                        continue;
                    };
                    if let Err(reason) = reduce_event(working, envelope) {
                        reduction_failures.insert(seq, reason.clone());
                        push_gap(&mut gaps, seq, seq, reason);
                        state = None;
                    }
                }
                ParsedEvent::Legacy(_) => {
                    let reason = "legacy event lacks typed before/after replay data".to_string();
                    reduction_failures.insert(seq, reason.clone());
                    push_gap(&mut gaps, seq, seq, reason);
                    state = None;
                }
                ParsedEvent::Unsupported(event) => {
                    reduction_failures.insert(seq, event.reason.clone());
                    push_gap(&mut gaps, seq, seq, event.reason.clone());
                    state = None;
                }
            }
        }
    }

    let projected_state = state
        .as_ref()
        .map(compose_replay_state)
        .transpose()
        .map_err(ReplayQueryError::Unavailable)?;
    let reconstruction_status = if projected_state.is_none() {
        "unavailable"
    } else if gaps.is_empty() {
        "available"
    } else {
        "partial"
    };
    let tree_revision = state
        .as_ref()
        .map(|state| state.tree_revision)
        .or_else(|| event_tree_revision_at(&inputs.events, at))
        .unwrap_or_default();

    let transactions = inputs
        .events
        .iter()
        .filter(|event| event.seq() > after && event.seq() <= at)
        .filter_map(|event| {
            let transaction =
                project_transaction(event, validations.get(&event.seq()), &reduction_failures);
            if request.branch.as_ref().is_some_and(|branch| {
                transaction.subject != *branch
                    && !transaction.affected_subjects.iter().any(|id| id == branch)
            }) {
                None
            } else {
                Some(transaction)
            }
        })
        .collect();

    Ok(ReplayResponse {
        schema_version: 1,
        meta: ReplayMeta {
            live_event_seq: inputs.marker.last_event_seq,
            at_event_seq: at,
            checkpoint_event_seq,
            earliest_replayable_seq,
            tree_revision,
            projected_at: now(),
        },
        reconstruction: ReplayReconstruction {
            status: reconstruction_status.to_string(),
            gaps,
        },
        state: projected_state,
        transactions,
    })
}

fn known_replay_branch(
    branch: &str,
    live: &ProjectMapProjection,
    events: &[ParsedEvent],
    validations: &HashMap<u64, CheckpointValidation>,
) -> bool {
    live.nodes.iter().any(|node| node.id == branch)
        || validations.values().any(|validation| {
            validation
                .checkpoint
                .as_ref()
                .is_some_and(|checkpoint| checkpoint.branches.iter().any(|item| item.id == branch))
        })
        || events.iter().any(|event| {
            let transaction_subject = match event {
                ParsedEvent::Current(event) => event.subject.as_str(),
                ParsedEvent::Legacy(event) => event.subject.as_str(),
                ParsedEvent::Unsupported(event) => event.subject.as_str(),
            };
            transaction_subject == branch
                || affected_subjects(event)
                    .iter()
                    .any(|subject| subject == branch)
        })
}

fn read_replay_inputs(root: &Path) -> Result<ReplayInputs, String> {
    let root = fs::canonicalize(root)
        .map_err(|error| format!("cannot resolve project root {}: {}", root.display(), error))?;
    ensure_lock_absent(&root)?;
    let treework = root.join(".TreeWork");
    let marker_path = treework.join("state/project.json");
    let marker_a = read_marker(&marker_path)?;
    let event_path = treework.join("events.jsonl");
    let metadata = fs::metadata(&event_path)
        .map_err(|error| format!("cannot inspect {}: {}", event_path.display(), error))?;
    if metadata.len() > MAX_REPLAY_EVENT_BYTES {
        return Err(format!(
            "event log exceeds the {} MiB Replay input limit",
            MAX_REPLAY_EVENT_BYTES / 1024 / 1024
        ));
    }
    let bytes = fs::read(&event_path)
        .map_err(|error| format!("cannot read {}: {}", event_path.display(), error))?;
    let events = crate::event::parse_event_log(&bytes)?;
    let marker_b = read_marker(&marker_path)?;
    ensure_lock_absent(&root)?;
    if marker_a != marker_b {
        return Err("publication marker changed during Replay read".to_string());
    }
    let event_tail = events.last().map(ParsedEvent::seq).unwrap_or_default();
    if event_tail != marker_b.last_event_seq {
        return Err(format!(
            "event tail sequence {} does not match publication marker {}",
            event_tail, marker_b.last_event_seq
        ));
    }
    if marker_b.schema_version != "0.1"
        || !matches!(
            marker_b.stage.as_str(),
            "alignment" | "build_tree" | "work_tree"
        )
        || marker_b.current_branch.trim().is_empty()
        || marker_b.last_sync.trim().is_empty()
    {
        return Err("publication marker contains unsupported Replay metadata".to_string());
    }
    Ok(ReplayInputs {
        marker: marker_b,
        events,
    })
}

fn read_marker(path: &Path) -> Result<ReplayMarker, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {}", path.display(), error))?;
    serde_json::from_str(&source)
        .map_err(|error| format!("cannot parse {}: {}", path.display(), error))
}

fn ensure_lock_absent(root: &Path) -> Result<(), String> {
    let lock = root.join(".TreeWork.lock");
    if lock.exists() {
        Err("TreeWork publication lock is active; Replay read was not started".to_string())
    } else {
        Ok(())
    }
}

fn validate_checkpoints(root: &Path, events: &[ParsedEvent]) -> HashMap<u64, CheckpointValidation> {
    events
        .iter()
        .filter_map(|event| {
            let ParsedEvent::Current(envelope) = event else {
                return None;
            };
            let reference = checkpoint_reference(envelope)?;
            let validation = match load_checkpoint(root, reference) {
                Ok(checkpoint) => match validate_checkpoint_event(envelope, &checkpoint) {
                    Ok(()) => CheckpointValidation {
                        checkpoint: Some(checkpoint),
                        error: None,
                    },
                    Err(error) => CheckpointValidation {
                        checkpoint: None,
                        error: Some(error),
                    },
                },
                Err(error) => CheckpointValidation {
                    checkpoint: None,
                    error: Some(format!(
                        "checkpoint for event {} is invalid: {}",
                        envelope.seq, error.0
                    )),
                },
            };
            Some((envelope.seq, validation))
        })
        .collect()
}

fn checkpoint_reference(event: &EventEnvelope) -> Option<&str> {
    match &event.data {
        EventData::ProjectInitialized(data) => Some(data.snapshot_ref.as_str()),
        EventData::TreeApplied(data) => Some(data.snapshot_ref.as_str()),
        _ => None,
    }
}

fn validate_checkpoint_event(
    event: &EventEnvelope,
    checkpoint: &TreeCheckpoint,
) -> Result<(), String> {
    if checkpoint.event_seq != event.seq || checkpoint.tree_revision != event.tree_revision {
        return Err(format!(
            "checkpoint identity does not match event {}",
            event.seq
        ));
    }
    match &event.data {
        EventData::ProjectInitialized(data) => {
            if checkpoint.checkpoint_hash != data.checkpoint_hash
                || checkpoint.tree_revision != 0
                || data.stage.before.is_some()
                || data.current_branch.before.is_some()
                || data.stage.after != "alignment"
                || data.current_branch.after != "root"
                || checkpoint.project.stage != data.stage.after
                || checkpoint.project.current_branch != data.current_branch.after
            {
                return Err(format!(
                    "genesis checkpoint does not match project.initialized event {}",
                    event.seq
                ));
            }
        }
        EventData::TreeApplied(data) => {
            let Some(tree) = checkpoint.tree.as_ref() else {
                return Err(format!(
                    "tree.apply checkpoint {} has no accepted Tree",
                    event.seq
                ));
            };
            if checkpoint.checkpoint_hash != data.checkpoint_hash
                || checkpoint.tree_revision != data.result.tree_revision
                || checkpoint.project.tree_hash != data.result.tree_document_hash
                || tree.state_hash != data.result.accepted_tree_state_hash
                || data.base.event_seq >= event.seq
                || (data.result.topology_changed
                    && data.result.tree_revision <= data.base.tree_revision)
                || (!data.result.topology_changed
                    && data.result.tree_revision != data.base.tree_revision)
            {
                return Err(format!(
                    "checkpoint does not match tree.apply event {}",
                    event.seq
                ));
            }
        }
        _ => {
            return Err(format!(
                "event {} does not own a Replay checkpoint",
                event.seq
            ))
        }
    }
    Ok(())
}

fn reduce_event(state: &mut WorkingState, event: &EventEnvelope) -> Result<(), String> {
    if event.tree_revision != state.tree_revision {
        return Err(format!(
            "event {} Tree revision {} does not match reconstructed revision {}",
            event.seq, event.tree_revision, state.tree_revision
        ));
    }
    match &event.data {
        EventData::ProjectInitialized(_) => {
            return Err("project.initialized appeared after the selected checkpoint".to_string())
        }
        EventData::AlignmentStarted(data) => {
            require_before(
                event.seq,
                "project.stage",
                &state.project.stage,
                &data.stage.before,
            )?;
            if data.stage.after != "alignment" {
                return Err(format!(
                    "event {} alignment.started has invalid stage `{}`",
                    event.seq, data.stage.after
                ));
            }
            state.project.stage = data.stage.after.clone();
        }
        EventData::AlignmentAccepted(data) => {
            require_before(
                event.seq,
                "project.stage",
                &state.project.stage,
                &data.stage.before,
            )?;
            if !matches!(data.stage.after.as_str(), "build_tree" | "work_tree") {
                return Err(format!(
                    "event {} alignment.accepted has invalid stage `{}`",
                    event.seq, data.stage.after
                ));
            }
            state.project.stage = data.stage.after.clone();
        }
        EventData::TreeEditingStarted(data) | EventData::TreeEditingUpdated(data) => {
            require_before(
                event.seq,
                "project.stage",
                &state.project.stage,
                &data.stage.before,
            )?;
            if data.stage.after != "build_tree"
                || !matches!(data.editing.mode.as_str(), "start" | "update")
                || data.editing.base_tree_revision > state.tree_revision
                || data.editing.base_event_seq > event.seq
                || data.editing.base_state_hash.trim().is_empty()
            {
                return Err(format!(
                    "event {} has invalid Tree Editing Session metadata",
                    event.seq
                ));
            }
            state.project.stage = data.stage.after.clone();
            state.project.tree_editing = Some(TreeEditingSession {
                mode: data.editing.mode.clone(),
                base_tree_revision: data.editing.base_tree_revision,
                base_event_seq: data.editing.base_event_seq,
                base_state_hash: data.editing.base_state_hash.clone(),
                opened_at: event.time.clone(),
            });
        }
        EventData::TreeApplied(_) => {
            return Err("tree.applied must be reduced through its checkpoint".to_string())
        }
        EventData::BranchEntered(data) => {
            if data.current_branch.after != event.subject
                || !state
                    .branches
                    .iter()
                    .any(|branch| branch.id == data.current_branch.after)
                || data.status.after != "in_progress"
            {
                return Err(format!(
                    "event {} has an invalid branch.entered target or status",
                    event.seq
                ));
            }
            require_before(
                event.seq,
                "project.current_branch",
                &state.project.current_branch,
                &data.current_branch.before,
            )?;
            let branch = state.branch_mut(&event.subject)?;
            require_before(
                event.seq,
                "branch.status",
                &branch.status,
                &data.status.before,
            )?;
            require_before(
                event.seq,
                "branch.status_reason",
                &branch.status_reason,
                &data.reason.before,
            )?;
            branch.status = data.status.after.clone();
            branch.status_reason = data.reason.after.clone();
            branch.isolation.mode = data.isolation.mode.clone();
            branch.isolation.workspace_path = data.isolation.workspace_path.clone();
            branch.isolation.git_branch = data.isolation.git_branch.clone();
            branch.isolation.managed_by_treework = data.isolation.managed_by_treework;
            branch.isolation.last_status = data.isolation.action.clone();
            state.project.current_branch = data.current_branch.after.clone();
        }
        EventData::BranchPaused(data) => {
            if data.status.after != "paused" {
                return Err(format!(
                    "event {} branch.paused has invalid status `{}`",
                    event.seq, data.status.after
                ));
            }
            let branch = state.branch_mut(&event.subject)?;
            require_before(
                event.seq,
                "branch.status",
                &branch.status,
                &data.status.before,
            )?;
            require_before(
                event.seq,
                "branch.status_reason",
                &branch.status_reason,
                &data.reason.before,
            )?;
            branch.status = data.status.after.clone();
            branch.status_reason = data.reason.after.clone();
        }
        EventData::BranchAborted(data) => {
            if data.status.after != "aborted" {
                return Err(format!(
                    "event {} branch.aborted has invalid status `{}`",
                    event.seq, data.status.after
                ));
            }
            let branch = state.branch_mut(&event.subject)?;
            require_before(
                event.seq,
                "branch.status",
                &branch.status,
                &data.status.before,
            )?;
            require_before(
                event.seq,
                "branch.status_reason",
                &branch.status_reason,
                &data.reason.before,
            )?;
            branch.status = data.status.after.clone();
            branch.status_reason = data.reason.after.clone();
        }
        EventData::BranchCompleted(data) => {
            if data.status.after != "complete" || !valid_verification(&data.verification.status) {
                return Err(format!(
                    "event {} branch.completed has invalid resulting state",
                    event.seq
                ));
            }
            let branch = state.branch_mut(&event.subject)?;
            require_before(
                event.seq,
                "branch.status",
                &branch.status,
                &data.status.before,
            )?;
            require_before(
                event.seq,
                "branch.status_reason",
                &branch.status_reason,
                &data.reason.before,
            )?;
            if branch.verification_status != data.verification.status {
                return Err(format!(
                    "event {} completion verification `{}` does not match reconstructed `{}`",
                    event.seq, data.verification.status, branch.verification_status
                ));
            }
            branch.status = data.status.after.clone();
            branch.status_reason = data.reason.after.clone();
        }
        EventData::VerificationRecorded(data) => {
            if !valid_verification(&data.verification.after) {
                return Err(format!(
                    "event {} verification.recorded has invalid verification `{}`",
                    event.seq, data.verification.after
                ));
            }
            let branch = state.branch_mut(&event.subject)?;
            require_before(
                event.seq,
                "branch.verification",
                &branch.verification_status,
                &data.verification.before,
            )?;
            branch.verification_status = data.verification.after.clone();
        }
    }
    Ok(())
}

fn valid_verification(value: &str) -> bool {
    matches!(value, "unverified" | "partial" | "verified" | "failed")
}

fn require_before(seq: u64, field: &str, actual: &str, expected: &str) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "event {} {} before-value `{}` does not match reconstructed `{}`",
            seq, field, expected, actual
        ))
    }
}

fn compose_replay_state(state: &WorkingState) -> Result<ReplayState, String> {
    let branch_by_id: HashMap<&str, &CheckpointBranch> = state
        .branches
        .iter()
        .map(|branch| (branch.id.as_str(), branch))
        .collect();
    let (topology_source, topology) = if state.tree_revision == 0 {
        let root = branch_by_id
            .get("root")
            .ok_or_else(|| "revision-zero Replay state has no root branch".to_string())?;
        (
            "bootstrap",
            vec![crate::tree_document::AcceptedTreeNode {
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
        let tree = state
            .tree
            .as_ref()
            .ok_or_else(|| "Replay state has no accepted Tree topology".to_string())?;
        ("accepted", tree.nodes.clone())
    };

    let mut child_counts = HashMap::new();
    for node in &topology {
        if !node.parent.is_empty() {
            *child_counts.entry(node.parent.as_str()).or_insert(0usize) += 1;
        }
    }
    let mut nodes = Vec::with_capacity(topology.len());
    for node in &topology {
        let branch = branch_by_id
            .get(node.id.as_str())
            .ok_or_else(|| format!("Replay topology node `{}` has no lifecycle state", node.id))?;
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
            is_current: state.project.current_branch == node.id,
            readiness: readiness(branch, &node.depends_on, &branch_by_id),
            depends_on: node.depends_on.clone(),
            child_count: child_counts.get(node.id.as_str()).copied().unwrap_or(0),
        });
    }
    let dependencies = topology
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
    Ok(ReplayState {
        tree_editing: state.project.tree_editing.is_some(),
        project: ProjectMapProject {
            stage: state.project.stage.clone(),
            current_branch: state.project.current_branch.clone(),
            topology_source: topology_source.to_string(),
        },
        nodes,
        dependencies,
    })
}

fn readiness(
    branch: &CheckpointBranch,
    dependencies: &[String],
    branches: &HashMap<&str, &CheckpointBranch>,
) -> String {
    match branch.status.as_str() {
        "in_progress" => "active",
        "complete" => "complete",
        "paused" => "paused",
        "aborted" => "aborted",
        "pending"
            if dependencies.iter().all(|dependency| {
                branches
                    .get(dependency.as_str())
                    .is_some_and(|item| item.status == "complete")
            }) =>
        {
            "ready"
        }
        _ => "waiting",
    }
    .to_string()
}

fn project_transaction(
    event: &ParsedEvent,
    checkpoint: Option<&CheckpointValidation>,
    reduction_failures: &HashMap<u64, String>,
) -> ReplayTransaction {
    let (time, event_type, subject, message, tree_revision, changes, base_reason) = match event {
        ParsedEvent::Current(event) => (
            event.time.clone(),
            event.event_type.clone(),
            event.subject.clone(),
            event.message.clone(),
            Some(event.tree_revision),
            event.data.to_value().unwrap_or(Value::Null),
            None,
        ),
        ParsedEvent::Legacy(event) => (
            event.time.clone(),
            event.event_type.clone(),
            event.subject.clone(),
            event.message.clone(),
            event.tree_revision,
            Value::Null,
            Some("legacy event lacks typed before/after replay data".to_string()),
        ),
        ParsedEvent::Unsupported(event) => (
            event.time.clone(),
            event.event_type.clone(),
            event.subject.clone(),
            event.message.clone(),
            event.tree_revision,
            Value::Null,
            Some(event.reason.clone()),
        ),
    };
    let checkpoint_reason = checkpoint.and_then(|validation| validation.error.clone());
    let reason = reduction_failures
        .get(&event.seq())
        .cloned()
        .or(checkpoint_reason)
        .or(base_reason);
    ReplayTransaction {
        seq: event.seq(),
        time,
        event_type,
        subject,
        message,
        tree_revision,
        affected_subjects: affected_subjects(event),
        changes,
        replayable: reason.is_none(),
        replayability_reason: reason,
    }
}

fn affected_subjects(event: &ParsedEvent) -> Vec<String> {
    let mut subjects = Vec::new();
    let ParsedEvent::Current(event) = event else {
        return subjects;
    };
    match &event.data {
        EventData::TreeApplied(data) => subjects.extend(data.affected_subjects.clone()),
        EventData::BranchEntered(data) => {
            subjects.push(event.subject.clone());
            subjects.push(data.current_branch.before.clone());
            subjects.push(data.current_branch.after.clone());
        }
        _ => subjects.push(event.subject.clone()),
    }
    let mut seen = HashSet::new();
    subjects.retain(|subject| !subject.is_empty() && seen.insert(subject.clone()));
    subjects
}

fn event_tree_revision_at(events: &[ParsedEvent], at: u64) -> Option<u64> {
    events
        .iter()
        .find(|event| event.seq() == at)
        .and_then(|event| match event {
            ParsedEvent::Current(event) => Some(event.tree_revision),
            ParsedEvent::Legacy(event) => event.tree_revision,
            ParsedEvent::Unsupported(event) => event.tree_revision,
        })
}

fn push_gap(gaps: &mut Vec<ReplayGap>, from_seq: u64, to_seq: u64, reason: String) {
    if let Some(previous) = gaps.last_mut() {
        if previous.to_seq.saturating_add(1) == from_seq && previous.reason == reason {
            previous.to_seq = to_seq;
            return;
        }
    }
    gaps.push(ReplayGap {
        from_seq,
        to_seq,
        reason,
    });
}

fn now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::write_checkpoint;
    use crate::event::{
        AlignmentData, BranchEnteredData, EventData, InitialTransition, IsolationEventData,
        ProjectInitializedData, Transition, TreeAppliedBase, TreeAppliedData, TreeAppliedResult,
        TreeEditingData, TreeEditingSummary,
    };
    use crate::tree_document::{AcceptedTreeNode, AcceptedTreeState};
    use crate::{accepted_tree_state_hash, Branch, BranchIsolation, BranchScope, Project};
    use std::path::PathBuf;
    use tempfile::TempDir;

    struct ReplayFixture {
        _temp: TempDir,
        root: PathBuf,
        live: ProjectMapProjection,
        events: Vec<EventEnvelope>,
        apply_checkpoint: PathBuf,
    }

    impl ReplayFixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().expect("temporary Replay project");
            let root = temp.path().join("project");
            fs::create_dir_all(root.join(".TreeWork/state")).expect("state directory");
            fs::create_dir_all(root.join(".TreeWork/history/checkpoints"))
                .expect("checkpoint directory");

            let root_branch = branch("root", "", "Root", "in_progress");
            let genesis_project = project("alignment", "root", 1, 0, "");
            let genesis = TreeCheckpoint::new(
                1,
                "unix:1".to_string(),
                &genesis_project,
                None,
                std::slice::from_ref(&root_branch),
            )
            .expect("genesis checkpoint");
            let (genesis_ref, genesis_hash) =
                write_checkpoint(&root, &genesis).expect("write genesis");
            let initialized = EventEnvelope::new(
                1,
                "unix:1".to_string(),
                "root",
                "TreeWork initialized",
                0,
                EventData::ProjectInitialized(ProjectInitializedData {
                    stage: InitialTransition {
                        before: None,
                        after: "alignment".to_string(),
                    },
                    current_branch: InitialTransition {
                        before: None,
                        after: "root".to_string(),
                    },
                    snapshot_ref: genesis_ref,
                    checkpoint_hash: genesis_hash,
                }),
            );

            let feature_branch = branch("feature", "root", "Feature", "pending");
            let mut tree = AcceptedTreeState {
                schema_version: 1,
                revision: 1,
                source_hash: "fnv1a64:tree-source".to_string(),
                state_hash: String::new(),
                accepted_at: "unix:2".to_string(),
                root: "root".to_string(),
                nodes: vec![
                    AcceptedTreeNode {
                        id: "root".to_string(),
                        parent: String::new(),
                        title: "Root".to_string(),
                        purpose: "Coordinate work.".to_string(),
                        spec: Some("spec.md".to_string()),
                        sibling_order: 0,
                        depends_on: Vec::new(),
                    },
                    AcceptedTreeNode {
                        id: "feature".to_string(),
                        parent: "root".to_string(),
                        title: "Feature".to_string(),
                        purpose: "Build the feature.".to_string(),
                        spec: Some("branches/feature/spec.md".to_string()),
                        sibling_order: 0,
                        depends_on: Vec::new(),
                    },
                ],
            };
            tree.state_hash = accepted_tree_state_hash(&tree).expect("tree hash");
            let apply_project = project("work_tree", "root", 2, 1, tree.source_hash.as_str());
            let apply_checkpoint = TreeCheckpoint::new(
                2,
                "unix:2".to_string(),
                &apply_project,
                Some(tree.clone()),
                &[root_branch.clone(), feature_branch.clone()],
            )
            .expect("Apply checkpoint");
            let (apply_ref, apply_hash) =
                write_checkpoint(&root, &apply_checkpoint).expect("write Apply checkpoint");
            let apply_checkpoint_path = root.join(".TreeWork").join(&apply_ref);
            let applied = EventEnvelope::new(
                2,
                "unix:2".to_string(),
                "root",
                "Applied declarative Tree",
                1,
                EventData::TreeApplied(TreeAppliedData {
                    base: TreeAppliedBase {
                        event_seq: 1,
                        tree_revision: 0,
                        state_hash: "bootstrap".to_string(),
                    },
                    result: TreeAppliedResult {
                        tree_revision: 1,
                        tree_document_hash: tree.source_hash.clone(),
                        accepted_tree_state_hash: tree.state_hash.clone(),
                        topology_changed: true,
                    },
                    operations: Vec::new(),
                    affected_subjects: vec!["feature".to_string()],
                    snapshot_ref: apply_ref,
                    checkpoint_hash: apply_hash,
                }),
            );
            let entered = EventEnvelope::new(
                3,
                "unix:3".to_string(),
                "feature",
                "Entered branch",
                1,
                EventData::BranchEntered(BranchEnteredData {
                    current_branch: Transition {
                        before: "root".to_string(),
                        after: "feature".to_string(),
                    },
                    status: Transition {
                        before: "pending".to_string(),
                        after: "in_progress".to_string(),
                    },
                    reason: Transition {
                        before: String::new(),
                        after: String::new(),
                    },
                    isolation: IsolationEventData {
                        mode: "none".to_string(),
                        workspace_path: String::new(),
                        git_branch: String::new(),
                        managed_by_treework: false,
                        action: "none".to_string(),
                    },
                }),
            );

            let events = vec![initialized, applied, entered];
            write_event_log(&root, &events);
            write_marker(&root, "work_tree", "feature", 3, 1, &tree.source_hash);
            let live = live_projection(3, 1);
            Self {
                _temp: temp,
                root,
                live,
                events,
                apply_checkpoint: apply_checkpoint_path,
            }
        }

        fn query(&self, request: ReplayRequest) -> Result<ReplayResponse, ReplayQueryError> {
            project_replay(&self.root, &self.live, request)
        }
    }

    #[test]
    fn seeks_and_reduces_to_an_arbitrary_sequence() {
        let fixture = ReplayFixture::new();
        let response = fixture
            .query(ReplayRequest {
                at: Some(3),
                ..ReplayRequest::default()
            })
            .expect("Replay response");
        assert_eq!(response.reconstruction.status, "available");
        assert_eq!(response.meta.checkpoint_event_seq, Some(2));
        let state = response.state.expect("reconstructed state");
        assert_eq!(state.project.current_branch, "feature");
        assert_eq!(state.nodes.len(), 2);
        assert!(state
            .nodes
            .iter()
            .any(|node| node.id == "feature" && node.status == "in_progress"));
    }

    #[test]
    fn branch_filter_never_filters_global_reconstruction() {
        let fixture = ReplayFixture::new();
        let response = fixture
            .query(ReplayRequest {
                branch: Some("feature".to_string()),
                ..ReplayRequest::default()
            })
            .expect("branch-filtered Replay");
        assert_eq!(response.state.expect("state").nodes.len(), 2);
        assert_eq!(
            response
                .transactions
                .iter()
                .map(|transaction| transaction.seq)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
    }

    #[test]
    fn branch_filter_accepts_a_historical_branch_absent_from_live_projection() {
        let fixture = ReplayFixture::new();
        let mut live = fixture.live.clone();
        live.nodes.retain(|node| node.id != "feature");
        let response = project_replay(
            &fixture.root,
            &live,
            ReplayRequest {
                branch: Some("feature".to_string()),
                ..ReplayRequest::default()
            },
        )
        .expect("historical branch Replay");
        assert_eq!(
            response
                .transactions
                .iter()
                .map(|transaction| transaction.seq)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
    }

    #[test]
    fn legacy_event_creates_an_honest_partial_gap() {
        let fixture = ReplayFixture::new();
        let mut source = fixture.events[0].to_json_line().expect("event");
        source.push_str(&fixture.events[1].to_json_line().expect("event"));
        source.push_str(
            "{\"seq\":3,\"time\":\"unix:3\",\"type\":\"branch.entered\",\"subject\":\"feature\",\"message\":\"legacy\",\"tree_revision\":1}\n",
        );
        fs::write(fixture.root.join(".TreeWork/events.jsonl"), source).expect("legacy log");
        let response = fixture
            .query(ReplayRequest::default())
            .expect("unavailable Replay");
        assert_eq!(response.reconstruction.status, "unavailable");
        assert!(response.state.is_none());
        assert_eq!(response.reconstruction.gaps[0].from_seq, 3);
        assert!(!response.transactions[2].replayable);
    }

    #[test]
    fn invalid_typed_transition_is_not_projected_as_available_state() {
        let fixture = ReplayFixture::new();
        let mut entered: Value =
            serde_json::from_str(fixture.events[2].to_json_line().expect("event").trim_end())
                .expect("event JSON");
        entered["data"]["status"]["after"] = Value::String("invented".to_string());
        let source = format!(
            "{}{}{}\n",
            fixture.events[0].to_json_line().expect("event"),
            fixture.events[1].to_json_line().expect("event"),
            serde_json::to_string(&entered).expect("event JSON")
        );
        fs::write(fixture.root.join(".TreeWork/events.jsonl"), source).expect("tampered log");
        let response = fixture
            .query(ReplayRequest::default())
            .expect("unavailable Replay");
        assert_eq!(response.reconstruction.status, "unavailable");
        assert!(response.state.is_none());
        assert!(!response.transactions[2].replayable);
    }

    #[test]
    fn alignment_transition_preserves_an_open_tree_editing_session() {
        let fixture = ReplayFixture::new();
        let validations = validate_checkpoints(
            &fixture.root,
            &fixture
                .events
                .iter()
                .cloned()
                .map(ParsedEvent::Current)
                .collect::<Vec<_>>(),
        );
        let checkpoint = validations
            .get(&2)
            .and_then(|validation| validation.checkpoint.as_ref())
            .expect("Apply checkpoint");
        let mut state = WorkingState::from_checkpoint(checkpoint);
        let editing = EventEnvelope::new(
            3,
            "unix:3".to_string(),
            "root",
            "Opened Tree update",
            1,
            EventData::TreeEditingUpdated(TreeEditingData {
                stage: Transition {
                    before: "work_tree".to_string(),
                    after: "build_tree".to_string(),
                },
                editing: TreeEditingSummary {
                    mode: "update".to_string(),
                    base_tree_revision: 1,
                    base_event_seq: 3,
                    base_state_hash: "fnv1a64:accepted".to_string(),
                },
            }),
        );
        reduce_event(&mut state, &editing).expect("editing transition");
        let alignment = EventEnvelope::new(
            4,
            "unix:4".to_string(),
            "root",
            "Alignment started",
            1,
            EventData::AlignmentStarted(AlignmentData {
                stage: Transition {
                    before: "build_tree".to_string(),
                    after: "alignment".to_string(),
                },
            }),
        );
        reduce_event(&mut state, &alignment).expect("Alignment transition");
        assert_eq!(state.project.stage, "alignment");
        assert_eq!(
            state
                .project
                .tree_editing
                .as_ref()
                .map(|session| session.mode.as_str()),
            Some("update")
        );
    }

    #[test]
    fn alignment_end_can_return_an_existing_tree_to_work_tree() {
        let fixture = ReplayFixture::new();
        let validations = validate_checkpoints(
            &fixture.root,
            &fixture
                .events
                .iter()
                .cloned()
                .map(ParsedEvent::Current)
                .collect::<Vec<_>>(),
        );
        let checkpoint = validations
            .get(&2)
            .and_then(|validation| validation.checkpoint.as_ref())
            .expect("Apply checkpoint");
        let mut state = WorkingState::from_checkpoint(checkpoint);
        let start = EventEnvelope::new(
            3,
            "unix:3".to_string(),
            "root",
            "Alignment started",
            1,
            EventData::AlignmentStarted(AlignmentData {
                stage: Transition {
                    before: "work_tree".to_string(),
                    after: "alignment".to_string(),
                },
            }),
        );
        reduce_event(&mut state, &start).expect("Alignment start");
        let end = EventEnvelope::new(
            4,
            "unix:4".to_string(),
            "root",
            "Alignment ended with user approval",
            1,
            EventData::AlignmentAccepted(AlignmentData {
                stage: Transition {
                    before: "alignment".to_string(),
                    after: "work_tree".to_string(),
                },
            }),
        );
        reduce_event(&mut state, &end).expect("Alignment end");
        assert_eq!(state.project.stage, "work_tree");
        assert!(state.project.tree_editing.is_none());
    }

    #[test]
    fn corrupt_apply_checkpoint_makes_post_apply_state_unavailable() {
        let fixture = ReplayFixture::new();
        let mut value: Value = serde_json::from_str(
            &fs::read_to_string(&fixture.apply_checkpoint).expect("checkpoint"),
        )
        .expect("checkpoint JSON");
        value["project"]["stage"] = Value::String("alignment".to_string());
        fs::write(
            &fixture.apply_checkpoint,
            format!("{}\n", serde_json::to_string_pretty(&value).expect("JSON")),
        )
        .expect("tamper checkpoint");
        let response = fixture
            .query(ReplayRequest {
                at: Some(2),
                ..ReplayRequest::default()
            })
            .expect("degraded coverage response");
        assert_eq!(response.reconstruction.status, "unavailable");
        assert!(response.state.is_none());
        assert!(!response.transactions[1].replayable);
    }

    #[test]
    fn invalid_ranges_and_unknown_branches_are_explicit() {
        let fixture = ReplayFixture::new();
        assert!(matches!(
            fixture.query(ReplayRequest {
                at: Some(4),
                ..ReplayRequest::default()
            }),
            Err(ReplayQueryError::BadRequest(_))
        ));
        assert!(matches!(
            fixture.query(ReplayRequest {
                at: Some(2),
                after: Some(3),
                ..ReplayRequest::default()
            }),
            Err(ReplayQueryError::BadRequest(_))
        ));
        assert!(matches!(
            fixture.query(ReplayRequest {
                branch: Some("missing".to_string()),
                ..ReplayRequest::default()
            }),
            Err(ReplayQueryError::NotFound(_))
        ));
    }

    fn project(
        stage: &str,
        current_branch: &str,
        last_event_seq: u64,
        tree_revision: u64,
        tree_hash: &str,
    ) -> Project {
        Project {
            schema_version: "0.1".to_string(),
            stage: stage.to_string(),
            current_branch: current_branch.to_string(),
            last_event_seq,
            tree_revision,
            tree_editing: None,
            tree_hash: tree_hash.to_string(),
            last_sync: format!("unix:{}", last_event_seq),
        }
    }

    fn branch(id: &str, parent: &str, title: &str, status: &str) -> Branch {
        Branch {
            path: id.to_string(),
            parent: parent.to_string(),
            title: title.to_string(),
            purpose: if id == "root" {
                "Coordinate work.".to_string()
            } else {
                "Build the feature.".to_string()
            },
            scope: BranchScope::default(),
            intake_rationale: String::new(),
            status: status.to_string(),
            verification_status: "unverified".to_string(),
            sync_status: "clean".to_string(),
            isolation: BranchIsolation::default(),
            status_reason: String::new(),
            last_sync: "unix:1".to_string(),
        }
    }

    fn write_event_log(root: &Path, events: &[EventEnvelope]) {
        let mut source = String::new();
        for event in events {
            source.push_str(&event.to_json_line().expect("serialize event"));
        }
        fs::write(root.join(".TreeWork/events.jsonl"), source).expect("event log");
    }

    fn write_marker(
        root: &Path,
        stage: &str,
        current_branch: &str,
        last_event_seq: u64,
        tree_revision: u64,
        tree_hash: &str,
    ) {
        let marker = serde_json::json!({
            "schema_version": "0.1",
            "stage": stage,
            "current_branch": current_branch,
            "last_event_seq": last_event_seq,
            "tree_revision": tree_revision,
            "tree_editing": null,
            "tree_hash": tree_hash,
            "last_sync": format!("unix:{}", last_event_seq)
        });
        fs::write(
            root.join(".TreeWork/state/project.json"),
            format!(
                "{}\n",
                serde_json::to_string_pretty(&marker).expect("marker JSON")
            ),
        )
        .expect("marker");
    }

    fn live_projection(event_seq: u64, tree_revision: u64) -> ProjectMapProjection {
        ProjectMapProjection {
            schema_version: 1,
            tree_revision,
            state_event_seq: event_seq,
            narrative_revision: "sha256:test".to_string(),
            tree_editing: false,
            projected_at: "unix:3".to_string(),
            health: crate::project_map_read_model::ProjectMapHealth {
                status: "ok".to_string(),
                message: String::new(),
            },
            project: ProjectMapProject {
                stage: "work_tree".to_string(),
                current_branch: "feature".to_string(),
                topology_source: "accepted".to_string(),
            },
            nodes: vec![
                ProjectMapNode {
                    id: "root".to_string(),
                    parent: String::new(),
                    order: 0,
                    title: "Root".to_string(),
                    purpose: "Coordinate work.".to_string(),
                    spec: Some("spec.md".to_string()),
                    status: "in_progress".to_string(),
                    verification: "unverified".to_string(),
                    status_reason: String::new(),
                    is_current: false,
                    readiness: "active".to_string(),
                    depends_on: Vec::new(),
                    child_count: 1,
                },
                ProjectMapNode {
                    id: "feature".to_string(),
                    parent: "root".to_string(),
                    order: 0,
                    title: "Feature".to_string(),
                    purpose: "Build the feature.".to_string(),
                    spec: Some("branches/feature/spec.md".to_string()),
                    status: "in_progress".to_string(),
                    verification: "unverified".to_string(),
                    status_reason: String::new(),
                    is_current: true,
                    readiness: "active".to_string(),
                    depends_on: Vec::new(),
                    child_count: 0,
                },
            ],
            dependencies: Vec::new(),
        }
    }
}
