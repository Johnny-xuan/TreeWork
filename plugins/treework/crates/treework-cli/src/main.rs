use clap::{Args, CommandFactory, Parser, Subcommand};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

mod branch_artifacts;
mod checkpoint;
mod event;
mod project_map_read_model;
mod project_map_replay;
mod project_map_server;
mod project_map_watcher;
mod transaction;
mod tree_diff;
mod tree_document;
mod tree_migration;
mod tree_transaction;

use checkpoint::{write_checkpoint, TreeCheckpoint};
use event::{
    AlignmentData, BranchCompletedData, BranchEnteredData, BranchStatusData, EventData,
    EventEnvelope, InitialTransition, IsolationEventData, ProjectInitializedData, Transition,
    TreeAppliedBase, TreeAppliedData, TreeAppliedResult, TreeEditingData, TreeEditingSummary,
    VerificationEvidence, VerificationRecordedData, VerificationSummary,
};
use transaction::{recover_pending_transaction, PublicationTransaction, RecoveryOutcome};
use tree_diff::{diff_tree, omitted_branch_ids};
use tree_document::{
    accepted_nodes, parse_tree_document, serialize_tree_document, AcceptedTreeNode,
    AcceptedTreeState, TreeDocument,
};
use tree_migration::{document_from_legacy, LegacyTreeNode};
use tree_transaction::{TreeApplyJournal, TreeApplyPlan};

const VERSION: &str = env!("TREEWORK_BUILD_VERSION");
const TW_DIR: &str = ".TreeWork";
const CONTROL_DESCRIPTOR: &str = "treework/control.json";
const WORKTREE_BRANCH_DESCRIPTOR: &str = "treework-branch.json";

static INVOCATION: OnceLock<Invocation> = OnceLock::new();

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Project {
    #[serde(default = "default_schema_version")]
    schema_version: String,
    #[serde(default = "default_stage")]
    stage: String,
    #[serde(default = "default_current_branch")]
    current_branch: String,
    #[serde(default)]
    last_event_seq: u64,
    #[serde(default)]
    tree_revision: u64,
    #[serde(default)]
    tree_editing: Option<TreeEditingSession>,
    #[serde(default, alias = "project_index_hash")]
    tree_hash: String,
    #[serde(default = "now")]
    last_sync: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TreeEditingSession {
    mode: String,
    base_tree_revision: u64,
    base_event_seq: u64,
    base_state_hash: String,
    opened_at: String,
}

#[derive(Clone, Debug)]
struct Invocation {
    control_root: PathBuf,
    workspace_root: PathBuf,
    branch: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ControlRootDescriptor {
    version: u32,
    project_id: String,
    control_root: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct WorktreeBranchBinding {
    #[serde(default = "default_binding_version")]
    version: u32,
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    branch: String,
    #[serde(default)]
    workspace: String,
}

impl Invocation {
    fn is_control_workspace(&self) -> bool {
        self.workspace_root == self.control_root
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Branch {
    #[serde(default)]
    path: String,
    #[serde(default)]
    parent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    purpose: String,
    #[serde(default, skip_serializing_if = "BranchScope::is_empty")]
    scope: BranchScope,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    intake_rationale: String,
    #[serde(default = "default_branch_status")]
    status: String,
    #[serde(default = "default_verification_status")]
    verification_status: String,
    #[serde(default = "default_sync_status")]
    sync_status: String,
    #[serde(default, skip_serializing_if = "BranchIsolation::is_empty")]
    isolation: BranchIsolation,
    #[serde(default, alias = "blocker", skip_serializing_if = "String::is_empty")]
    status_reason: String,
    #[serde(default)]
    last_sync: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct BranchIsolation {
    #[serde(default)]
    mode: String,
    #[serde(default)]
    workspace_path: String,
    #[serde(default)]
    git_branch: String,
    #[serde(default)]
    managed_by_treework: bool,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    last_entered_at: String,
    #[serde(default)]
    last_status: String,
}

impl BranchIsolation {
    fn is_empty(&self) -> bool {
        self.mode.is_empty()
            && self.workspace_path.is_empty()
            && self.git_branch.is_empty()
            && !self.managed_by_treework
            && self.created_at.is_empty()
            && self.last_entered_at.is_empty()
            && self.last_status.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Edge {
    #[serde(default)]
    id: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
    #[serde(default = "default_edge_kind")]
    kind: String,
    #[serde(default)]
    user_label: String,
    #[serde(default)]
    interpreted_relation: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct BranchState {
    #[serde(default)]
    branches: Vec<Branch>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GraphState {
    #[serde(default)]
    edges: Vec<Edge>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
struct BranchScope {
    #[serde(default)]
    accepts: Vec<String>,
    #[serde(default)]
    excludes: Vec<String>,
}

impl BranchScope {
    fn is_empty(&self) -> bool {
        self.accepts.is_empty() && self.excludes.is_empty()
    }
}

#[derive(Clone, Copy, Default)]
struct BranchPlanChanges {
    title: bool,
    rationale: bool,
}

impl BranchPlanChanges {
    fn all() -> Self {
        Self {
            title: true,
            rationale: true,
        }
    }
}

#[derive(Serialize)]
struct GraphProjection {
    meta: GraphProjectionMeta,
    nodes: Vec<GraphNode>,
    edges: Vec<GraphProjectionEdge>,
}

#[derive(Serialize)]
struct GraphProjectionMeta {
    current_branch: String,
    stage: String,
    tree_revision: u64,
    last_event_seq: u64,
    last_sync: String,
    layout: GraphLayoutMeta,
}

#[derive(Clone, Serialize)]
struct GraphLayoutMeta {
    algorithm: String,
    x_spacing: f64,
    y_spacing: f64,
    max_depth: usize,
    node_count: usize,
    edge_count: usize,
}

#[derive(Serialize)]
struct GraphNode {
    id: String,
    parent: String,
    title: String,
    purpose: String,
    accepts: Vec<String>,
    excludes: Vec<String>,
    intake_rationale: String,
    status: String,
    verification: String,
    sync_status: String,
    status_reason: String,
    last_sync: String,
    layout: GraphNodeLayout,
}

#[derive(Clone, Serialize)]
struct GraphNodeLayout {
    x: f64,
    y: f64,
    depth: usize,
    order: usize,
    subtree_size: usize,
}

#[derive(Serialize)]
struct GraphProjectionEdge {
    id: String,
    from: String,
    to: String,
    kind: String,
    label: String,
}

#[derive(Serialize)]
struct BranchRecall {
    schema_version: String,
    generated_at: String,
    tree_revision: u64,
    publication_marker: RecallPublicationMarker,
    project: RecallProject,
    branch: Branch,
    parent: Option<Branch>,
    children: Vec<Branch>,
    related_edges: Vec<Edge>,
    isolation: BranchIsolationSummary,
    docs: BranchDocs,
    verification: BranchVerification,
    allowed_actions: Vec<String>,
    blocked_actions: Vec<RecallBlockedAction>,
}

#[derive(Serialize)]
struct RecallPublicationMarker {
    last_event_seq: u64,
    tree_revision: u64,
    tree_hash: String,
}

#[derive(Serialize)]
struct RecallBlockedAction {
    action: String,
    reason_codes: Vec<String>,
    reasons: Vec<String>,
}

#[derive(Serialize)]
struct BranchIsolationSummary {
    mode: String,
    workspace_path: String,
    git_branch: String,
    managed_by_treework: bool,
    exists: bool,
    clean: Option<bool>,
    last_status: String,
}

#[derive(Serialize)]
struct RecallProject {
    stage: String,
    current_branch: String,
}

#[derive(Serialize)]
struct BranchDocs {
    spec: String,
    task_plan: String,
    progress: String,
    findings: String,
    verification: String,
}

struct RecallActionBlocker {
    code: String,
    reason: String,
}

#[derive(Serialize)]
struct BranchVerification {
    status: String,
    acceptance_complete: bool,
    verification_doc_present: bool,
    coverage_gap: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct BranchWorkspaceGit {
    available: bool,
    root: String,
    current_branch: String,
    head: String,
    worktree_supported: bool,
}

#[derive(Debug)]
pub(crate) struct AppError(String);

impl From<io::Error> for AppError {
    fn from(err: io::Error) -> Self {
        AppError(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError(err.to_string())
    }
}

pub(crate) type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Parser)]
#[command(name = "tw", version = VERSION, about = "TreeWork project-state transaction CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<TwCommand>,
}

#[derive(Debug, Subcommand)]
enum TwCommand {
    Init,
    #[command(hide = true)]
    Check(BriefArgs),
    #[command(hide = true)]
    Sync,
    Align {
        #[command(subcommand)]
        command: AlignCommand,
    },
    Tree {
        #[command(subcommand)]
        command: TreeCommand,
    },
    #[command(alias = "cd")]
    Enter(EnterArgs),
    Recall(RecallArgs),
    Pause(PauseArgs),
    Abort(AbortArgs),
    Verify(VerifyArgs),
    Complete(CompleteArgs),
    #[command(hide = true)]
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },
    Version,
}

impl TwCommand {
    fn needs_lock(&self) -> bool {
        !matches!(
            self,
            TwCommand::Version
                | TwCommand::Graph {
                    command: GraphCommand::Serve(_)
                }
        )
    }
}

#[derive(Debug, Args)]
struct BriefArgs {
    #[arg(long)]
    brief: bool,
}

#[derive(Debug, Subcommand)]
enum AlignCommand {
    Start,
    #[command(about = "End Alignment after explicit user approval")]
    End,
}

#[derive(Debug, Subcommand)]
enum TreeCommand {
    #[command(about = "Open the first declarative Tree Editing Session")]
    Start,
    #[command(about = "Open a declarative Tree Editing Session from accepted state")]
    Update,
    #[command(about = "Validate and atomically apply .TreeWork/tree.yaml")]
    Apply,
}

#[derive(Debug, Args)]
struct EnterArgs {
    branch: String,
    #[arg(long)]
    dry_run: bool,
    #[arg(long = "no-isolate")]
    no_isolate: bool,
    #[arg(long)]
    recall: bool,
}

#[derive(Debug, Args)]
struct RecallArgs {
    branch: Option<String>,
    #[arg(long)]
    brief: bool,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct PauseArgs {
    #[arg(long)]
    reason: Option<String>,
}

#[derive(Debug, Args)]
struct AbortArgs {
    #[arg(long)]
    reason: String,
}

#[derive(Debug, Args)]
struct VerifyArgs {
    #[arg(long = "cmd", default_value = "not recorded")]
    command: String,
    #[arg(long, default_value = "partial")]
    result: String,
    #[arg(long, default_value = "not recorded")]
    gap: String,
}

#[derive(Debug, Args)]
struct CompleteArgs {
    #[arg(long = "keep-worktree")]
    keep_worktree: bool,
}

#[derive(Debug, Subcommand)]
enum GraphCommand {
    Render,
    Serve(GraphServeArgs),
}

#[derive(Debug, Args)]
struct GraphServeArgs {
    #[arg(long, default_value_t = 8765)]
    port: u16,
    #[arg(long)]
    once: bool,
}

struct LockGuard {
    path: PathBuf,
    owner: String,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if fs::read_to_string(self.path.join("owner.pid"))
            .is_ok_and(|owner| owner.trim() == self.owner)
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn main() {
    if let Err(err) = run() {
        eprintln!("TreeWork error: {}", err.0);
        std::process::exit(1);
    }
}

fn run() -> AppResult<()> {
    let cli = Cli::parse();
    let Some(command) = cli.command else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };
    let cwd = env::current_dir()?;
    let invocation = discover_invocation(&cwd)?;
    let root = invocation.control_root.clone();
    INVOCATION
        .set(invocation)
        .map_err(|_| AppError("TreeWork invocation was initialized twice".to_string()))?;
    let needs_lock = command.needs_lock();
    let _lock = if needs_lock {
        Some(acquire_lock(&root)?)
    } else {
        None
    };
    if needs_lock && existing_tw(&root).is_some() {
        recover_pending_transaction(&root)?;
        rollback_pending_tree_apply(&root)?;
    }
    match command {
        TwCommand::Init => {
            require_control_workspace("initialize TreeWork")?;
            cmd_init(&root)?
        }
        TwCommand::Check(args) => cmd_check(&root, args.brief)?,
        TwCommand::Sync => {
            require_control_workspace("synchronize project-wide projections")?;
            cmd_sync(&root)?
        }
        TwCommand::Align { command } => {
            require_control_workspace("change Alignment state")?;
            cmd_align(&root, command)?
        }
        TwCommand::Tree { command } => {
            require_control_workspace("change the project tree")?;
            cmd_tree(&root, command)?
        }
        TwCommand::Enter(args) => cmd_enter(&root, &args)?,
        TwCommand::Recall(args) => cmd_recall(&root, &args)?,
        TwCommand::Pause(args) => cmd_pause(&root, &args)?,
        TwCommand::Abort(args) => cmd_abort(&root, &args)?,
        TwCommand::Verify(args) => cmd_verify(&root, &args)?,
        TwCommand::Complete(args) => cmd_complete(&root, &args)?,
        TwCommand::Graph { command } => cmd_graph(&root, command)?,
        TwCommand::Version => println!("tw {}", VERSION),
    }
    Ok(())
}

fn cmd_init(root: &Path) -> AppResult<()> {
    ensure_control_descriptor(root)?;
    if tw_dir(root).join("state/project.json").exists() {
        println!(
            "TreeWork is already initialized at {}",
            tw_dir(root).display()
        );
        return Ok(());
    }

    let transaction = PublicationTransaction::begin(root, "project.initialize", &[], true)?;
    let result = (|| {
        scaffold_treework(root)?;
        let timestamp = now();
        let project = Project {
            schema_version: "0.1".to_string(),
            stage: "alignment".to_string(),
            current_branch: "root".to_string(),
            last_event_seq: 1,
            tree_revision: 0,
            tree_editing: None,
            tree_hash: String::new(),
            last_sync: timestamp.clone(),
        };
        let branches = vec![initial_root_branch(&timestamp)];
        let checkpoint = TreeCheckpoint::new(1, timestamp.clone(), &project, None, &branches)?;
        let (snapshot_ref, checkpoint_hash) = write_checkpoint(root, &checkpoint)?;
        inject_transaction_failure("transaction-after-checkpoint", &[])?;

        save_branches(root, &branches)?;
        save_edges(root, &[])?;
        sync_all_from_state(root, &project, &branches)?;
        inject_transaction_failure("transaction-after-accepted-state", &[])?;

        let event = EventEnvelope::new(
            1,
            timestamp,
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
                snapshot_ref: snapshot_ref.clone(),
                checkpoint_hash: checkpoint_hash.clone(),
            }),
        );
        publish_event_and_marker(
            root,
            transaction,
            &project,
            &event,
            Some((&snapshot_ref, &checkpoint_hash)),
        )
    })();
    settle_transaction_result(root, result)?;
    println!("Initialized TreeWork at {}", tw_dir(root).display());
    Ok(())
}

fn cmd_align(root: &Path, command: AlignCommand) -> AppResult<()> {
    match command {
        AlignCommand::Start => {
            if !tw_dir(root).join("state/project.json").exists() {
                return cmd_init(root);
            }
            let mut project = load_project(root)?;
            if project.stage == "alignment" {
                println!("Alignment is already active.");
                return Ok(());
            }
            if project.tree_editing.is_some() {
                return Err(AppError(
                    "cannot start Alignment while a Tree Editing Session is open; apply the current Tree candidate first"
                        .to_string(),
                ));
            }
            let previous_stage = project.stage.clone();
            let paths = vec![
                tw_dir(root).join("state/project.json"),
                tw_dir(root).join("events.jsonl"),
                tw_dir(root).join("progress.md"),
                tw_dir(root).join("requirements.md"),
                tw_dir(root).join("assumptions.md"),
                tw_dir(root).join("references.md"),
                tw_dir(root).join("idea_inbox.md"),
            ];
            let transaction =
                PublicationTransaction::begin(root, "alignment.start", &paths, false)?;
            project.stage = "alignment".to_string();
            project.last_sync = now();
            project.last_event_seq += 1;
            let result = (|| {
                write_alignment_templates(root)?;
                let branches = load_branches(root)?;
                sync_all_from_state(root, &project, &branches)?;
                inject_transaction_failure("transaction-after-accepted-state", &[])?;
                let event = EventEnvelope::new(
                    project.last_event_seq,
                    project.last_sync.clone(),
                    "root",
                    "Alignment started",
                    project.tree_revision,
                    EventData::AlignmentStarted(AlignmentData {
                        stage: Transition {
                            before: previous_stage,
                            after: project.stage.clone(),
                        },
                    }),
                );
                publish_event_and_marker(root, transaction, &project, &event, None)
            })();
            settle_transaction_result(root, result)?;
            println!("Alignment started.");
        }
        AlignCommand::End => {
            require_treework(root)?;
            let mut project = load_project(root)?;
            if project.stage != "alignment" {
                println!("Alignment is not active; no state changed.");
                return Ok(());
            }
            if project.tree_editing.is_some() {
                return Err(AppError(
                    "cannot end Alignment while project state still contains an open Tree Editing Session"
                        .to_string(),
                ));
            }
            let previous_stage = project.stage.clone();
            let next_stage = if project.tree_revision == 0 {
                "build_tree"
            } else {
                "work_tree"
            };
            let paths = vec![
                tw_dir(root).join("state/project.json"),
                tw_dir(root).join("events.jsonl"),
                tw_dir(root).join("progress.md"),
            ];
            let transaction = PublicationTransaction::begin(root, "alignment.end", &paths, false)?;
            project.stage = next_stage.to_string();
            project.last_sync = now();
            project.last_event_seq += 1;
            let result = (|| {
                let branches = load_branches(root)?;
                sync_all_from_state(root, &project, &branches)?;
                inject_transaction_failure("transaction-after-accepted-state", &[])?;
                let event = EventEnvelope::new(
                    project.last_event_seq,
                    project.last_sync.clone(),
                    "root",
                    if project.stage == "build_tree" {
                        "Alignment ended with user approval; first Tree planning is ready"
                    } else {
                        "Alignment ended with user approval; returning to the accepted Tree"
                    },
                    project.tree_revision,
                    EventData::AlignmentAccepted(AlignmentData {
                        stage: Transition {
                            before: previous_stage,
                            after: project.stage.clone(),
                        },
                    }),
                );
                publish_event_and_marker(root, transaction, &project, &event, None)
            })();
            settle_transaction_result(root, result)?;
            println!(
                "Alignment ended with user approval. Stage is now {}.",
                project.stage
            );
        }
    }
    Ok(())
}

fn cmd_tree(root: &Path, command: TreeCommand) -> AppResult<()> {
    match command {
        TreeCommand::Start => open_tree_editing_session(root, "start")?,
        TreeCommand::Update => open_tree_editing_session(root, "update")?,
        TreeCommand::Apply => apply_declarative_tree(root)?,
    }
    Ok(())
}

fn open_tree_editing_session(root: &Path, mode: &str) -> AppResult<()> {
    require_treework(root)?;
    let mut project = load_project(root)?;
    if project.tree_editing.is_some() {
        return Err(AppError(
            "a Tree Editing Session is already open; edit `.TreeWork/tree.yaml` and run `tw tree apply`"
                .to_string(),
        ));
    }
    match mode {
        "start" => {
            if project.stage == "alignment" {
                return Err(AppError(
                    "Alignment is still open; run `tw align end` only after the user approves the Alignment Review"
                        .to_string(),
                ));
            }
            if project.stage != "build_tree" {
                return Err(AppError(format!(
                    "tree start requires stage `build_tree`; current stage is `{}`",
                    project.stage
                )));
            }
            if project.tree_revision > 0 {
                return Err(AppError(
                    "the project already has an accepted tree; use `tw tree update`".to_string(),
                ));
            }
        }
        "update" => {
            if project.stage != "work_tree" {
                return Err(AppError(format!(
                    "tree update requires stage `work_tree`; current stage is `{}`",
                    project.stage
                )));
            }
        }
        _ => {
            return Err(AppError(format!(
                "unsupported tree editing mode `{}`",
                mode
            )))
        }
    }

    let previous_stage = project.stage.clone();
    let paths = vec![
        tw_dir(root).join("state/project.json"),
        tw_dir(root).join("state/tree.json"),
        tw_dir(root).join("events.jsonl"),
        tw_dir(root).join("tree.yaml"),
        tw_dir(root).join("archive"),
        tw_dir(root).join("progress.md"),
    ];
    let transaction =
        PublicationTransaction::begin(root, &format!("tree.editing.{}", mode), &paths, false)?;
    let result = (|| {
        ensure_tree_document_draft(root, &mut project)?;
        let branches = load_branches(root)?;
        let edges = load_edges(root)?;
        let timestamp = now();
        let next_event_seq = project.last_event_seq + 1;
        let editing = TreeEditingSession {
            mode: mode.to_string(),
            base_tree_revision: project.tree_revision,
            base_event_seq: next_event_seq,
            base_state_hash: accepted_state_hash(&project, &branches, &edges)?,
            opened_at: timestamp.clone(),
        };
        project.stage = "build_tree".to_string();
        project.tree_editing = Some(editing.clone());
        project.last_event_seq = next_event_seq;
        project.last_sync = timestamp;
        sync_all_from_state(root, &project, &branches)?;
        inject_transaction_failure("transaction-after-accepted-state", &[])?;
        let data = TreeEditingData {
            stage: Transition {
                before: previous_stage,
                after: project.stage.clone(),
            },
            editing: TreeEditingSummary {
                mode: editing.mode,
                base_tree_revision: editing.base_tree_revision,
                base_event_seq: editing.base_event_seq,
                base_state_hash: editing.base_state_hash,
            },
        };
        let event = EventEnvelope::new(
            project.last_event_seq,
            project.last_sync.clone(),
            "root",
            if mode == "start" {
                "Opened first declarative Tree Editing Session"
            } else {
                "Opened declarative Tree update session"
            },
            project.tree_revision,
            if mode == "start" {
                EventData::TreeEditingStarted(data)
            } else {
                EventData::TreeEditingUpdated(data)
            },
        );
        publish_event_and_marker(root, transaction, &project, &event, None)
    })();
    settle_transaction_result(root, result)?;
    println!(
        "Tree Editing Session opened. Edit and review `.TreeWork/tree.yaml`, then run `tw tree apply`."
    );
    Ok(())
}

fn tree_document_path(root: &Path) -> PathBuf {
    tw_dir(root).join("tree.yaml")
}

fn accepted_tree_path(root: &Path) -> PathBuf {
    tw_dir(root).join("state/tree.json")
}

fn ensure_tree_document_draft(root: &Path, project: &mut Project) -> AppResult<()> {
    let legacy_topology = !accepted_tree_path(root).exists()
        && (project.tree_revision > 0
            || (project.stage == "work_tree" && load_branches(root)?.len() > 1));
    if legacy_topology {
        archive_legacy_tree_document(root)?;
        migrate_legacy_tree_state(root, project)?;
        return Ok(());
    }
    if tree_document_path(root).exists() {
        let source = read_to_string(&tree_document_path(root))?;
        if parse_tree_document(&source).is_ok() {
            return Ok(());
        }
        if project.tree_revision == 0 {
            return Err(AppError(
                "existing `.TreeWork/tree.yaml` is not a valid declarative Tree; fix it before opening Build Tree"
                    .to_string(),
            ));
        }
        archive_legacy_tree_document(root)?;
        migrate_legacy_tree_state(root, project)?;
        return Ok(());
    }

    if project.tree_revision > 0 {
        migrate_legacy_tree_state(root, project)
    } else {
        write_template_if_missing(root, "tree.yaml", "tree.yaml")
    }
}

fn migrate_legacy_tree_state(root: &Path, project: &mut Project) -> AppResult<()> {
    let branches = load_branches(root)?;
    let edges = load_edges(root)?;
    archive_legacy_tree_state(root)?;
    let unsupported_relations: Vec<&str> = edges
        .iter()
        .filter(|edge| !matches!(edge.kind.as_str(), "parent_of" | "depends_on"))
        .map(|edge| edge.kind.as_str())
        .collect();
    let dependencies: HashMap<&str, Vec<String>> = branches
        .iter()
        .map(|branch| {
            let values = edges
                .iter()
                .filter(|edge| edge.kind == "depends_on" && edge.from == branch.path)
                .map(|edge| edge.to.clone())
                .collect();
            (branch.path.as_str(), values)
        })
        .collect();
    let legacy_nodes: Vec<LegacyTreeNode> = branches
        .iter()
        .map(|branch| {
            let default_spec = if branch.path == "root" {
                tw_dir(root)
                    .join("spec.md")
                    .exists()
                    .then(|| "spec.md".to_string())
            } else {
                let relative = format!("branches/{}/spec.md", branch.path);
                tw_dir(root).join(&relative).exists().then_some(relative)
            };
            LegacyTreeNode {
                id: branch.path.clone(),
                parent: branch.parent.clone(),
                title: if branch.title.trim().is_empty() {
                    branch_title_from_id(&branch.path)
                } else {
                    branch.title.clone()
                },
                purpose: if branch.purpose.trim().is_empty() {
                    if branch.path == "root" {
                        "Project-wide coordination and integration.".to_string()
                    } else {
                        format!("Own the {} work.", branch_title_from_id(&branch.path))
                    }
                } else {
                    branch.purpose.clone()
                },
                spec: default_spec,
                depends_on: dependencies
                    .get(branch.path.as_str())
                    .cloned()
                    .unwrap_or_default(),
            }
        })
        .collect();
    let document = document_from_legacy(&legacy_nodes).map_err(|errors| {
        AppError(format!(
            "cannot migrate the accepted legacy tree:\n{}",
            errors
                .iter()
                .map(|error| format!("- {}", error))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    })?;
    let source = serialize_tree_document(&document)
        .map_err(|error| AppError(format!("cannot serialize migrated Tree: {}", error)))?;
    let source_hash = stable_hash_str(&source);
    if project.tree_revision == 0 {
        project.tree_revision = 1;
    }
    let mut accepted = accepted_tree_from_document(
        &document,
        project.tree_revision,
        &source_hash,
        &project.last_sync,
    )?;
    accepted.state_hash = accepted_tree_state_hash(&accepted)?;
    write_atomic(&tree_document_path(root), &source)?;
    write_json_pretty(&accepted_tree_path(root), &accepted)?;
    project.tree_hash = source_hash;
    println!("Migrated legacy topology to `.TreeWork/tree.yaml` and `state/tree.json`.");
    if !unsupported_relations.is_empty() {
        let mut kinds: Vec<&str> = unsupported_relations
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        kinds.sort();
        println!(
            "Migration note: archived and omitted unsupported legacy relation kind(s): {}.",
            kinds.join(", ")
        );
    }
    Ok(())
}

fn archive_legacy_tree_state(root: &Path) -> AppResult<()> {
    let archive_dir = tw_dir(root).join("archive");
    fs::create_dir_all(&archive_dir)?;
    for (source, target) in [
        (
            "state/project-index.json",
            "project-index.pre-declarative.json",
        ),
        ("state/graph.json", "graph.pre-declarative.json"),
    ] {
        let source_path = tw_dir(root).join(source);
        if source_path.exists() {
            write_file_if_missing(&archive_dir.join(target), &read_to_string(&source_path)?)?;
        }
    }
    Ok(())
}

fn archive_legacy_tree_document(root: &Path) -> AppResult<()> {
    let path = tree_document_path(root);
    if !path.exists() {
        return Ok(());
    }
    let archive_dir = tw_dir(root).join("archive");
    fs::create_dir_all(&archive_dir)?;
    let mut target = archive_dir.join("tree.yaml.legacy");
    if target.exists() {
        target = archive_dir.join(format!("tree.yaml.legacy-{}", now().replace(':', "-")));
    }
    fs::rename(path, target)?;
    Ok(())
}

fn accepted_tree_from_document(
    document: &TreeDocument,
    revision: u64,
    source_hash: &str,
    accepted_at: &str,
) -> AppResult<AcceptedTreeState> {
    let mut state = AcceptedTreeState {
        schema_version: 1,
        revision,
        source_hash: source_hash.to_string(),
        state_hash: String::new(),
        accepted_at: accepted_at.to_string(),
        root: "root".to_string(),
        nodes: accepted_nodes(document),
    };
    state.state_hash = accepted_tree_state_hash(&state)?;
    Ok(state)
}

pub(crate) fn accepted_tree_state_hash(state: &AcceptedTreeState) -> AppResult<String> {
    let value = json!({
        "schema_version": state.schema_version,
        "revision": state.revision,
        "source_hash": state.source_hash,
        "root": state.root,
        "nodes": state.nodes,
    });
    Ok(stable_hash_str(&serde_json::to_string(&value)?))
}

fn accepted_state_hash(
    project: &Project,
    branches: &[Branch],
    edges: &[Edge],
) -> AppResult<String> {
    let value = json!({
        "project": {
            "schema_version": project.schema_version,
            "current_branch": project.current_branch,
            "tree_revision": project.tree_revision,
            "tree_hash": project.tree_hash,
        },
        "branches": branches,
        "edges": edges,
    });
    let serialized = serde_json::to_string(&value)
        .map_err(|err| AppError(format!("failed to serialize accepted state: {}", err)))?;
    Ok(stable_hash_str(&serialized))
}

fn build_declarative_tree_plan(root: &Path) -> AppResult<TreeApplyPlan> {
    require_build_tree_stage(root)?;
    let project = load_project(root)?;
    let session = project.tree_editing.clone().ok_or_else(|| {
        AppError(
            "no Tree Editing Session is open; run `tw tree start` or `tw tree update`".to_string(),
        )
    })?;
    let source = read_to_string(&tree_document_path(root))?;
    let source_hash = stable_hash_str(&source);
    let document = parse_tree_document(&source).map_err(|errors| {
        AppError(format!(
            "Tree document cannot be applied:\n{}",
            errors
                .iter()
                .map(|error| format!("- {}", error.render(".TreeWork/tree.yaml")))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    })?;
    let nodes = accepted_nodes(&document);
    let branches = load_branches(root)?;
    let edges = load_edges(root)?;
    let accepted_before = if accepted_tree_path(root).exists() {
        Some(load_accepted_tree(root)?)
    } else {
        None
    };
    let mut errors = Vec::new();

    if session.base_tree_revision != project.tree_revision {
        errors.push(format!(
            "stale Tree Editing Session: base revision is {}, accepted revision is {}",
            session.base_tree_revision, project.tree_revision
        ));
    }
    if session.base_event_seq != project.last_event_seq {
        errors.push(format!(
            "stale Tree Editing Session: base event is {}, current event is {}",
            session.base_event_seq, project.last_event_seq
        ));
    }
    let state_hash = accepted_state_hash(&project, &branches, &edges)?;
    if session.base_state_hash != state_hash {
        errors.push(format!(
            "stale Tree Editing Session: accepted state changed from {} to {}",
            session.base_state_hash, state_hash
        ));
    }

    if project.tree_revision > 0 && accepted_before.is_none() {
        errors.push("accepted tree revision has no `state/tree.json` snapshot".to_string());
    }
    if let Some(accepted) = &accepted_before {
        if accepted.schema_version != 1 {
            errors.push(format!(
                "accepted Tree schema version {} is unsupported",
                accepted.schema_version
            ));
        }
        if accepted.revision != project.tree_revision {
            errors.push(format!(
                "accepted Tree revision {} does not match project revision {}",
                accepted.revision, project.tree_revision
            ));
        }
        if accepted.source_hash != project.tree_hash {
            errors.push("accepted Tree source hash does not match project state".to_string());
        }
        let computed = accepted_tree_state_hash(accepted)?;
        if accepted.state_hash != computed {
            errors.push(format!(
                "accepted Tree state hash is invalid: expected {}, found {}",
                computed, accepted.state_hash
            ));
        }
    }

    for omitted in omitted_branch_ids(accepted_before.as_ref(), &nodes) {
        errors.push(format!(
            "accepted branch `{}` is missing from tree.yaml; omission cannot delete history",
            omitted
        ));
    }

    let candidate_by_id: HashMap<&str, &AcceptedTreeNode> =
        nodes.iter().map(|node| (node.id.as_str(), node)).collect();
    let accepted_by_id: HashMap<&str, &AcceptedTreeNode> = accepted_before
        .as_ref()
        .map(|tree| {
            tree.nodes
                .iter()
                .map(|node| (node.id.as_str(), node))
                .collect()
        })
        .unwrap_or_default();
    for branch in branches
        .iter()
        .filter(|branch| branch_is_structurally_protected(branch))
    {
        let (Some(previous), Some(candidate)) = (
            accepted_by_id.get(branch.path.as_str()),
            candidate_by_id.get(branch.path.as_str()),
        ) else {
            continue;
        };
        let mut protected_changes = Vec::new();
        if previous.parent != candidate.parent {
            protected_changes.push("parent");
        }
        if previous.title != candidate.title {
            protected_changes.push("title");
        }
        if previous.purpose != candidate.purpose {
            protected_changes.push("purpose");
        }
        if previous.spec != candidate.spec {
            protected_changes.push("spec");
        }
        if previous.depends_on != candidate.depends_on {
            protected_changes.push("depends_on");
        }
        if !protected_changes.is_empty() {
            errors.push(format!(
                "protected branch `{}` cannot change {} while `{}`",
                branch.path,
                protected_changes.join(", "),
                branch.status
            ));
        }
    }

    for node in &nodes {
        if let Some(spec) = &node.spec {
            if let Err(error) = validate_spec_target(root, spec) {
                errors.push(format!("branch `{}`: {}", node.id, error.0));
            }
        }
    }

    let operations = diff_tree(accepted_before.as_ref(), &nodes);
    Ok(TreeApplyPlan {
        document,
        nodes,
        source,
        source_hash,
        session,
        operations,
        errors,
    })
}

fn apply_declarative_tree(root: &Path) -> AppResult<()> {
    rollback_pending_tree_apply(root)?;
    let plan = build_declarative_tree_plan(root)?;
    if !plan.errors.is_empty() {
        return Err(AppError(format!(
            "Tree document cannot be applied:\n{}",
            plan.errors
                .iter()
                .map(|error| format!("- {}", error))
                .collect::<Vec<_>>()
                .join("\n")
        )));
    }
    let operation_count = plan.operations.len();
    let first_tree = plan.session.base_tree_revision == 0;
    apply_declarative_tree_inner(root, plan)?;
    println!(
        "Applied declarative Tree with {} semantic change(s). Stage is now work_tree.",
        operation_count
    );
    if first_tree {
        println!(
            "First Tree accepted. Call `treework_project_map` for `{}` and open its localhost URL in the Codex in-app browser.",
            root.display()
        );
    }
    Ok(())
}

fn apply_declarative_tree_inner(root: &Path, plan: TreeApplyPlan) -> AppResult<()> {
    let branches_dir = tw_dir(root).join("branches");
    let mut tracked_paths = vec![
        tw_dir(root).join("state/project.json"),
        tw_dir(root).join("state/branches.json"),
        tw_dir(root).join("state/graph.json"),
        accepted_tree_path(root),
        tw_dir(root).join("events.jsonl"),
        branches_dir.clone(),
        tw_dir(root).join("history"),
        tw_dir(root).join("progress.md"),
        tree_document_path(root),
    ];
    for relative in plan.nodes.iter().filter_map(|node| node.spec.as_deref()) {
        let path = tw_dir(root).join(relative);
        if !path.starts_with(&branches_dir) {
            tracked_paths.push(path);
        }
    }
    let transaction = PublicationTransaction::begin(root, "tree.apply", &tracked_paths, false)?;
    let result = (|| {
        if env::var("TREEWORK_TEST_FAILPOINT").as_deref() == Ok("tree-apply-mutate-source") {
            let mut changed = plan.source.clone();
            changed.push_str("\n# injected concurrent edit\n");
            write_atomic(&tree_document_path(root), &changed)?;
        }
        if read_to_string(&tree_document_path(root))? != plan.source {
            return Err(AppError(
                ".TreeWork/tree.yaml changed while Apply was preparing; review the latest file and run `tw tree apply` again"
                    .to_string(),
            ));
        }

        let mut project = load_project(root)?;
        let previous_branches = load_branches(root)?;
        let previous_edges = load_edges(root)?;
        let mut branches = Vec::with_capacity(plan.nodes.len());
        let timestamp = now();

        for node in &plan.nodes {
            if let Some(current) = previous_branches
                .iter()
                .find(|branch| branch.path == node.id)
            {
                let mut branch = current.clone();
                let old_parent = branch.parent.clone();
                let title_changed = branch.title != node.title;
                branch.parent = node.parent.clone();
                branch.title = node.title.clone();
                branch.purpose = node.purpose.clone();
                branch.last_sync = timestamp.clone();
                if node.id != "root" {
                    create_branch_docs(root, &node.id, &node.parent)?;
                    if old_parent != node.parent {
                        rewrite_branch_doc_headers(root, &node.id, &node.parent)?;
                    }
                    if title_changed {
                        sync_branch_plan_to_task_plan(
                            root,
                            &branch,
                            BranchPlanChanges {
                                title: title_changed,
                                ..BranchPlanChanges::default()
                            },
                        )?;
                    }
                }
                ensure_spec_document(root, node)?;
                branches.push(branch);
                continue;
            }

            let branch = Branch {
                path: node.id.clone(),
                parent: node.parent.clone(),
                title: node.title.clone(),
                purpose: node.purpose.clone(),
                scope: BranchScope::default(),
                intake_rationale: "Created from declarative `.TreeWork/tree.yaml`.".to_string(),
                status: "pending".to_string(),
                verification_status: "unverified".to_string(),
                sync_status: "clean".to_string(),
                isolation: BranchIsolation::default(),
                status_reason: String::new(),
                last_sync: timestamp.clone(),
            };
            if node.id != "root" {
                create_branch_docs(root, &branch.path, &branch.parent)?;
                sync_branch_plan_to_task_plan(root, &branch, BranchPlanChanges::all())?;
            }
            ensure_spec_document(root, node)?;
            branches.push(branch);
        }

        let mut edges: Vec<Edge> = previous_edges
            .iter()
            .filter(|edge| matches!(edge.kind.as_str(), "parent_of" | "depends_on"))
            .cloned()
            .collect();
        repair_parent_edges(&mut edges, &branches);
        let desired_dependencies: HashSet<(String, String)> = plan
            .nodes
            .iter()
            .flat_map(|node| {
                node.depends_on
                    .iter()
                    .map(|dependency| (node.id.clone(), dependency.clone()))
            })
            .collect();
        edges.retain(|edge| {
            edge.kind != "depends_on"
                || desired_dependencies.contains(&(edge.from.clone(), edge.to.clone()))
        });
        let mut seen_dependencies = HashSet::new();
        edges.retain(|edge| {
            edge.kind != "depends_on"
                || seen_dependencies.insert((edge.from.clone(), edge.to.clone()))
        });
        for (branch, dependency) in &desired_dependencies {
            if let Some(edge) = edges.iter_mut().find(|edge| {
                edge.kind == "depends_on" && edge.from == *branch && edge.to == *dependency
            }) {
                edge.user_label = format!("{} depends on {}", branch, dependency);
                edge.interpreted_relation = "depends_on".to_string();
            } else {
                let id = next_edge_id(&edges);
                edges.push(Edge {
                    id,
                    from: branch.clone(),
                    to: dependency.clone(),
                    kind: "depends_on".to_string(),
                    user_label: format!("{} depends on {}", branch, dependency),
                    interpreted_relation: "depends_on".to_string(),
                });
            }
        }

        let topology_changed = project.tree_revision == 0 || !plan.operations.is_empty();
        if topology_changed {
            project.tree_revision += 1;
        }
        project.stage = "work_tree".to_string();
        project.tree_editing = None;
        project.tree_hash = plan.source_hash.clone();
        project.last_event_seq += 1;
        project.last_sync = timestamp.clone();
        let accepted = accepted_tree_from_document(
            &plan.document,
            project.tree_revision,
            &plan.source_hash,
            &timestamp,
        )?;
        let checkpoint = TreeCheckpoint::new(
            project.last_event_seq,
            timestamp.clone(),
            &project,
            Some(accepted.clone()),
            &branches,
        )?;
        let (snapshot_ref, checkpoint_hash) = write_checkpoint(root, &checkpoint)?;
        inject_transaction_failure("transaction-after-checkpoint", &[])?;

        save_branches(root, &branches)?;
        save_edges(root, &edges)?;
        write_json_pretty(&accepted_tree_path(root), &accepted)?;
        sync_all_from_state(root, &project, &branches)?;
        let validation_findings =
            validate_prepared_state(&project, &branches, &edges, Some(&accepted))?;
        if !validation_findings.is_empty() {
            return Err(AppError(format!(
                "prepared Tree state failed internal validation:\n{}",
                validation_findings
                    .iter()
                    .map(|finding| format!("- {}", finding))
                    .collect::<Vec<_>>()
                    .join("\n")
            )));
        }
        inject_transaction_failure("transaction-after-accepted-state", &[])?;

        let mut affected_subjects: Vec<String> = plan
            .operations
            .iter()
            .map(|operation| operation.subject().to_string())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        affected_subjects.sort();
        let event = EventEnvelope::new(
            project.last_event_seq,
            timestamp,
            "root",
            if topology_changed {
                format!(
                    "Applied declarative Tree with {} semantic change(s)",
                    plan.operations.len()
                )
            } else {
                "Applied declarative Tree with no semantic changes".to_string()
            },
            project.tree_revision,
            EventData::TreeApplied(TreeAppliedData {
                base: TreeAppliedBase {
                    event_seq: plan.session.base_event_seq,
                    tree_revision: plan.session.base_tree_revision,
                    state_hash: plan.session.base_state_hash.clone(),
                },
                result: TreeAppliedResult {
                    tree_revision: project.tree_revision,
                    tree_document_hash: plan.source_hash.clone(),
                    accepted_tree_state_hash: accepted.state_hash.clone(),
                    topology_changed,
                },
                operations: plan.operations.clone(),
                affected_subjects,
                snapshot_ref: snapshot_ref.clone(),
                checkpoint_hash: checkpoint_hash.clone(),
            }),
        );
        publish_event_and_marker(
            root,
            transaction,
            &project,
            &event,
            Some((&snapshot_ref, &checkpoint_hash)),
        )
    })();
    settle_transaction_result(root, result)?;
    let _ = render_graph(root);
    Ok(())
}

fn load_accepted_tree(root: &Path) -> AppResult<AcceptedTreeState> {
    read_json(&accepted_tree_path(root))
}

fn validate_spec_target(root: &Path, relative: &str) -> AppResult<()> {
    let target = tw_dir(root).join(relative);
    let canonical_tw = canonical_existing(&tw_dir(root), ".TreeWork directory")?;
    let mut cursor = tw_dir(root);
    for component in Path::new(relative).components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(AppError(format!(
                "Spec path `{}` must stay inside `.TreeWork/`",
                relative
            )));
        };
        cursor.push(segment);
        if cursor.exists() {
            let metadata = fs::symlink_metadata(&cursor)?;
            if metadata.file_type().is_symlink() {
                return Err(AppError(format!(
                    "Spec path `{}` crosses symlink `{}`",
                    relative,
                    cursor.display()
                )));
            }
        }
    }
    if target.exists() {
        let canonical_target = canonical_existing(&target, "Spec path")?;
        if !canonical_target.starts_with(&canonical_tw) {
            return Err(AppError(format!(
                "Spec path `{}` escapes `.TreeWork/`",
                relative
            )));
        }
    }
    Ok(())
}

fn ensure_spec_document(root: &Path, node: &AcceptedTreeNode) -> AppResult<()> {
    let Some(relative) = &node.spec else {
        return Ok(());
    };
    validate_spec_target(root, relative)?;
    let template = if node.id == "root" {
        load_template("root_spec.md")?
    } else {
        load_template("branch_spec.md")?
            .replace("<branch path>", &node.id)
            .replace("<parent branch>", &node.parent)
    };
    write_file_if_missing(&tw_dir(root).join(relative), &template)
}

fn branch_title_from_id(id: &str) -> String {
    id.rsplit('/')
        .next()
        .unwrap_or(id)
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn require_build_tree_stage(root: &Path) -> AppResult<()> {
    let project = load_project(root)?;
    if project.stage != "build_tree" {
        return Err(AppError(format!(
            "tree apply requires stage `build_tree`; current stage is `{}`. Run `tw tree start` for the first pass or `tw tree update` from Work Tree",
            project.stage
        )));
    }
    Ok(())
}

fn rollback_pending_tree_apply(root: &Path) -> AppResult<()> {
    let journal_path = tree_apply_journal_path(root);
    if !journal_path.exists() {
        return Ok(());
    }
    let journal: TreeApplyJournal = read_json(&journal_path)?;
    save_project(root, &journal.old_project)?;
    save_branches(root, &journal.old_branches)?;
    save_edges(root, &journal.old_edges)?;
    write_atomic(&tw_dir(root).join("events.jsonl"), &journal.old_events)?;
    match journal.old_tree_state {
        Some(content) => write_atomic(&accepted_tree_path(root), &content)?,
        None => {
            let path = accepted_tree_path(root);
            if path.exists() {
                fs::remove_file(path)?;
            }
        }
    }
    let branch_backup = tree_apply_branch_backup_dir(root);
    if branch_backup.exists() {
        let branches_dir = tw_dir(root).join("branches");
        if branches_dir.exists() {
            fs::remove_dir_all(&branches_dir)?;
        }
        copy_dir_recursive(&branch_backup, &branches_dir)?;
        fs::remove_dir_all(branch_backup)?;
    }
    for backup in &journal.file_backups {
        let path = tw_dir(root).join(&backup.relative_path);
        match &backup.old_content {
            Some(content) => write_atomic(&path, content)?,
            None => {
                if path.exists() {
                    fs::remove_file(&path)?;
                    remove_empty_parents(path.parent(), &tw_dir(root))?;
                }
            }
        }
    }
    fs::remove_file(journal_path)?;
    sync_all(root)?;
    let _ = render_graph(root);
    Ok(())
}

fn remove_empty_parents(mut current: Option<&Path>, stop: &Path) -> AppResult<()> {
    while let Some(path) = current {
        if path == stop || !path.starts_with(stop) {
            break;
        }
        if fs::read_dir(path)?.next().is_some() {
            break;
        }
        fs::remove_dir(path)?;
        current = path.parent();
    }
    Ok(())
}

fn tree_apply_journal_path(root: &Path) -> PathBuf {
    tw_dir(root).join("state/pending-tree-apply.json")
}

fn tree_apply_branch_backup_dir(root: &Path) -> PathBuf {
    tw_dir(root).join("state/pending-tree-apply-branches")
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
    if !src.exists() {
        return Ok(());
    }
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let entry_path = entry.path();
        let target_path = dst.join(entry.file_name());
        if entry_path.is_dir() {
            copy_dir_recursive(&entry_path, &target_path)?;
        } else {
            if let Some(parent) = target_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&entry_path, &target_path)?;
        }
    }
    Ok(())
}

fn cmd_enter(root: &Path, args: &EnterArgs) -> AppResult<()> {
    require_treework(root)?;
    let branch_path = args.branch.as_str();
    let mut project = load_project(root)?;
    if let Some(bound_branch) = &invocation().branch {
        if bound_branch != branch_path {
            return Err(AppError(format!(
                "TreeWork workspace error: this worktree is bound to `{}` and may not enter `{}`",
                bound_branch, branch_path
            )));
        }
    } else if !invocation().is_control_workspace() {
        return Err(AppError(format!(
            "TreeWork workspace error: linked worktree `{}` has no branch binding",
            invocation().workspace_root.display()
        )));
    }
    let mut branches = load_branches(root)?;
    let branch_index = branches
        .iter()
        .position(|branch| branch.path == branch_path)
        .ok_or_else(|| AppError(format!("missing branch `{}`", branch_path)))?;
    let before_branch = branches[branch_index].clone();
    if matches!(before_branch.status.as_str(), "complete" | "aborted") {
        return Err(AppError(format!(
            "cannot enter branch `{}` with terminal status `{}`",
            before_branch.path, before_branch.status
        )));
    }

    if args.dry_run {
        let mut branch_preview = before_branch.clone();
        let isolation = prepare_enter_isolation(root, &mut branch_preview, args)?;
        println!("Dry-run enter `{}`.", branch_path);
        print_enter_isolation(&isolation);
        if args.recall {
            let recall = build_branch_recall(root, &project, branch_path)?;
            println!();
            print!("{}", render_branch_recall_markdown(&recall, false));
        }
        return Ok(());
    }

    let paths = vec![
        tw_dir(root).join("state/project.json"),
        tw_dir(root).join("state/branches.json"),
        tw_dir(root).join("events.jsonl"),
        tw_dir(root).join("progress.md"),
        branch_dir(root, branch_path).join("progress.md"),
    ];
    let mut transaction = PublicationTransaction::begin(root, "branch.enter", &paths, false)?;
    let mut candidate = before_branch.clone();
    let isolation = match prepare_enter_isolation(root, &mut candidate, args) {
        Ok(outcome) => outcome,
        Err(error) => return settle_transaction_result(root, Err(error)),
    };
    if isolation.executed {
        if let Err(error) =
            transaction.record_created_worktree(&isolation.workspace_path, &isolation.git_branch)
        {
            return recover_enter_error(root, error, &isolation);
        }
    }
    if isolation.mode == "git-worktree" && !isolation.workspace_path.trim().is_empty() {
        if let Err(error) =
            write_workspace_binding(root, Path::new(&isolation.workspace_path), branch_path)
        {
            return recover_enter_error(root, error, &isolation);
        }
    }
    candidate.status = "in_progress".to_string();
    candidate.status_reason.clear();
    let resulting_current = if invocation().is_control_workspace() {
        branch_path.to_string()
    } else {
        project.current_branch.clone()
    };
    if project.current_branch == resulting_current
        && branch_enter_visible_state(&before_branch) == branch_enter_visible_state(&candidate)
    {
        recover_pending_transaction(root)?;
        println!(
            "Branch `{}` is already current and in progress.",
            branch_path
        );
        print_enter_isolation(&isolation);
        return Ok(());
    }

    let previous_current = project.current_branch.clone();
    let timestamp = now();
    candidate.last_sync = timestamp.clone();
    branches[branch_index] = candidate.clone();
    project.current_branch = resulting_current;
    project.last_sync = timestamp.clone();
    project.last_event_seq += 1;
    let result = (|| {
        save_branches(root, &branches)?;
        sync_all_from_state(root, &project, &branches)?;
        inject_transaction_failure("transaction-after-accepted-state", &[])?;
        let event = EventEnvelope::new(
            project.last_event_seq,
            timestamp,
            branch_path,
            format!("Entered branch ({})", isolation.status),
            project.tree_revision,
            EventData::BranchEntered(BranchEnteredData {
                current_branch: Transition {
                    before: previous_current,
                    after: project.current_branch.clone(),
                },
                status: Transition {
                    before: before_branch.status.clone(),
                    after: candidate.status.clone(),
                },
                reason: Transition {
                    before: before_branch.status_reason.clone(),
                    after: candidate.status_reason.clone(),
                },
                isolation: IsolationEventData {
                    mode: candidate.isolation.mode.clone(),
                    workspace_path: candidate.isolation.workspace_path.clone(),
                    git_branch: candidate.isolation.git_branch.clone(),
                    managed_by_treework: candidate.isolation.managed_by_treework,
                    action: isolation.action.clone(),
                },
            }),
        );
        publish_event_and_marker(root, transaction, &project, &event, None)
    })();
    settle_enter_transaction(root, result, &isolation)?;
    println!("Entered branch `{}`.", branch_path);
    print_enter_isolation(&isolation);
    println!("Recall: tw recall {}", branch_path);
    if args.recall {
        let recall = build_branch_recall(root, &project, branch_path)?;
        println!();
        print!("{}", render_branch_recall_markdown(&recall, false));
    }
    Ok(())
}

fn cmd_recall(root: &Path, args: &RecallArgs) -> AppResult<()> {
    require_treework(root)?;
    let project = load_project(root)?;
    let branch_path = args
        .branch
        .as_deref()
        .or_else(|| invocation().branch.as_deref())
        .unwrap_or(&project.current_branch)
        .to_string();
    let recall = build_branch_recall(root, &project, &branch_path)?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&recall)?);
    } else {
        print!("{}", render_branch_recall_markdown(&recall, args.brief));
    }
    Ok(())
}

struct BranchIsolationOutcome {
    mode: String,
    workspace_path: String,
    git_branch: String,
    status: String,
    action: String,
    warning: String,
    executed: bool,
}

fn prepare_enter_isolation(
    root: &Path,
    branch: &mut Branch,
    args: &EnterArgs,
) -> AppResult<BranchIsolationOutcome> {
    if args.no_isolate {
        if !args.dry_run {
            branch.isolation.last_entered_at = now();
            branch.isolation.last_status = "skipped by --no-isolate".to_string();
        }
        return Ok(BranchIsolationOutcome {
            mode: "none".to_string(),
            workspace_path: String::new(),
            git_branch: String::new(),
            status: "isolation skipped by --no-isolate".to_string(),
            action: "skipped".to_string(),
            warning: String::new(),
            executed: false,
        });
    }

    let git = detect_workspace_git(root);
    if !git.available || !git.worktree_supported {
        if !args.dry_run {
            branch.isolation.last_entered_at = now();
            branch.isolation.last_status =
                "no git worktree support; stayed in current workspace".to_string();
        }
        return Ok(BranchIsolationOutcome {
            mode: "none".to_string(),
            workspace_path: String::new(),
            git_branch: String::new(),
            status: "no isolation workspace prepared".to_string(),
            action: "unavailable".to_string(),
            warning: "Git worktree support was not detected; continue in the current workspace or rerun with --no-isolate.".to_string(),
            executed: false,
        });
    }

    let workspace_path = if branch.isolation.workspace_path.trim().is_empty() {
        default_branch_worktree_path(root, &branch.path)
    } else {
        PathBuf::from(branch.isolation.workspace_path.trim())
    };
    let git_branch = if branch.isolation.git_branch.trim().is_empty() {
        default_branch_git_branch(&branch.path)
    } else {
        branch.isolation.git_branch.clone()
    };
    let workspace_exists = workspace_path.exists();

    if args.dry_run {
        return Ok(BranchIsolationOutcome {
            mode: "git-worktree".to_string(),
            workspace_path: workspace_path.display().to_string(),
            git_branch,
            status: if workspace_exists {
                "would reuse existing worktree".to_string()
            } else {
                "would create managed worktree".to_string()
            },
            action: if workspace_exists {
                "would_reuse".to_string()
            } else {
                "would_create".to_string()
            },
            warning: "CLI cannot change the parent shell cwd; the caller should move to the printed workspace path after enter.".to_string(),
            executed: false,
        });
    }

    if workspace_exists {
        branch.isolation.last_status = "reused existing worktree".to_string();
    } else {
        if let Some(parent) = workspace_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let base_ref = if !git.current_branch.trim().is_empty() && git.current_branch != "HEAD" {
            git.current_branch.clone()
        } else if !git.head.trim().is_empty() {
            git.head.clone()
        } else {
            "HEAD".to_string()
        };
        let branch_exists = git_ref_exists(root, &git_branch);
        let mut command_args = vec![
            "worktree".to_string(),
            "add".to_string(),
            workspace_path.display().to_string(),
        ];
        if branch_exists {
            command_args.push(git_branch.clone());
        } else {
            command_args.push("-b".to_string());
            command_args.push(git_branch.clone());
            command_args.push(base_ref);
        }
        let output = command_capture(root, "git", &command_args)?;
        if !output.success {
            return Err(AppError(format!(
                "failed to prepare git worktree `{}`: {}",
                workspace_path.display(),
                output.stderr_or_stdout()
            )));
        }
        branch.isolation.created_at = now();
        branch.isolation.last_status = "created managed worktree".to_string();
    }

    branch.isolation.mode = "git-worktree".to_string();
    branch.isolation.workspace_path = workspace_path.display().to_string();
    branch.isolation.git_branch = git_branch.clone();
    branch.isolation.managed_by_treework = true;
    branch.isolation.last_entered_at = now();

    Ok(BranchIsolationOutcome {
        mode: "git-worktree".to_string(),
        workspace_path: workspace_path.display().to_string(),
        git_branch,
        status: branch.isolation.last_status.clone(),
        action: if workspace_exists {
            "reused".to_string()
        } else {
            "created".to_string()
        },
        warning: "CLI cannot change the parent shell cwd; move to the printed workspace path if your environment did not switch automatically.".to_string(),
        executed: !workspace_exists,
    })
}

fn print_enter_isolation(outcome: &BranchIsolationOutcome) {
    println!("Isolation:");
    println!("  mode: {}", outcome.mode);
    if !outcome.workspace_path.trim().is_empty() {
        println!("  workspace: {}", outcome.workspace_path);
    }
    if !outcome.git_branch.trim().is_empty() {
        println!("  git branch: {}", outcome.git_branch);
    }
    println!("  status: {}", outcome.status);
    println!("  executed: {}", outcome.executed);
    if !outcome.warning.trim().is_empty() {
        println!("  note: {}", outcome.warning);
    }
}

#[derive(PartialEq, Eq)]
struct BranchEnterVisibleState {
    status: String,
    status_reason: String,
    isolation_mode: String,
    workspace_path: String,
    git_branch: String,
    managed_by_treework: bool,
    isolation_status: String,
}

fn branch_enter_visible_state(branch: &Branch) -> BranchEnterVisibleState {
    BranchEnterVisibleState {
        status: branch.status.clone(),
        status_reason: branch.status_reason.clone(),
        isolation_mode: branch.isolation.mode.clone(),
        workspace_path: branch.isolation.workspace_path.clone(),
        git_branch: branch.isolation.git_branch.clone(),
        managed_by_treework: branch.isolation.managed_by_treework,
        isolation_status: branch.isolation.last_status.clone(),
    }
}

fn settle_enter_transaction(
    root: &Path,
    result: AppResult<()>,
    isolation: &BranchIsolationOutcome,
) -> AppResult<()> {
    let Err(error) = result else {
        return Ok(());
    };
    recover_enter_error(root, error, isolation)
}

fn recover_enter_error(
    root: &Path,
    error: AppError,
    isolation: &BranchIsolationOutcome,
) -> AppResult<()> {
    match recover_pending_transaction(root) {
        Ok(RecoveryOutcome::RolledBack) if isolation.executed => {
            if let Err(cleanup_error) = cleanup_created_enter_isolation(root, isolation) {
                Err(AppError(format!(
                    "{}; accepted state rolled back, but cleanup of newly created worktree failed: {}",
                    error.0, cleanup_error.0
                )))
            } else {
                Err(error)
            }
        }
        Ok(RecoveryOutcome::FinishedForward) => Ok(()),
        Ok(_) => Err(error),
        Err(recovery_error) => Err(AppError(format!(
            "{}; transaction recovery also failed: {}",
            error.0, recovery_error.0
        ))),
    }
}

fn cleanup_created_enter_isolation(
    root: &Path,
    isolation: &BranchIsolationOutcome,
) -> AppResult<()> {
    if !isolation.executed || isolation.workspace_path.trim().is_empty() {
        return Ok(());
    }
    if Path::new(&isolation.workspace_path).exists() {
        let remove = command_capture(
            root,
            "git",
            &[
                "worktree".to_string(),
                "remove".to_string(),
                "--force".to_string(),
                isolation.workspace_path.clone(),
            ],
        )?;
        if !remove.success {
            return Err(AppError(format!(
                "git worktree remove failed for `{}`: {}",
                isolation.workspace_path,
                remove.stderr_or_stdout()
            )));
        }
    }
    if !isolation.git_branch.trim().is_empty() && git_ref_exists(root, &isolation.git_branch) {
        let remove_branch = command_capture(
            root,
            "git",
            &[
                "branch".to_string(),
                "-D".to_string(),
                isolation.git_branch.clone(),
            ],
        )?;
        if !remove_branch.success {
            return Err(AppError(format!(
                "git branch cleanup failed for `{}`: {}",
                isolation.git_branch,
                remove_branch.stderr_or_stdout()
            )));
        }
    }
    Ok(())
}

fn branch_publication_paths(root: &Path, branch: &str, verification: bool) -> Vec<PathBuf> {
    let active_docs = docs_dir_for_branch(root, branch);
    let control_docs = if branch == "root" {
        tw_dir(root)
    } else {
        branch_dir(root, branch)
    };
    let mut paths = vec![
        tw_dir(root).join("state/project.json"),
        tw_dir(root).join("state/branches.json"),
        tw_dir(root).join("events.jsonl"),
        tw_dir(root).join("progress.md"),
        active_docs.join("progress.md"),
        control_docs.join("progress.md"),
    ];
    if verification {
        paths.push(active_docs.join("verification.md"));
        paths.push(control_docs.join("verification.md"));
    }
    paths
}

fn cmd_pause(root: &Path, args: &PauseArgs) -> AppResult<()> {
    require_treework(root)?;
    let mut project = load_project(root)?;
    let mut branches = load_branches(root)?;
    let current_branch = resolve_mutation_target(root, &project, None)?;
    let branch_index = branches
        .iter()
        .position(|branch| branch.path == current_branch)
        .ok_or_else(|| AppError(format!("missing branch `{}`", current_branch)))?;
    let before = branches[branch_index].clone();
    if matches!(before.status.as_str(), "complete" | "aborted") {
        return Err(AppError(format!(
            "cannot pause branch `{}` with terminal status `{}`",
            before.path, before.status
        )));
    }
    let requested_reason = args.reason.clone().unwrap_or_default().trim().to_string();
    if before.status == "paused" && before.status_reason == requested_reason {
        println!("Branch `{}` is already paused.", current_branch);
        return Ok(());
    }
    let paths = branch_publication_paths(root, &current_branch, false);
    let transaction = PublicationTransaction::begin(root, "branch.pause", &paths, false)?;
    let timestamp = now();
    let branch = &mut branches[branch_index];
    branch.status = "paused".to_string();
    branch.status_reason = requested_reason;
    branch.last_sync = timestamp.clone();
    let event_message = if branch.status_reason.is_empty() {
        "Branch paused".to_string()
    } else {
        branch.status_reason.clone()
    };
    let after = branch.clone();
    project.last_event_seq += 1;
    project.last_sync = timestamp.clone();
    let result = (|| {
        save_branches(root, &branches)?;
        sync_all_from_state(root, &project, &branches)?;
        inject_transaction_failure("transaction-after-accepted-state", &[])?;
        let event = EventEnvelope::new(
            project.last_event_seq,
            timestamp,
            &current_branch,
            event_message,
            project.tree_revision,
            EventData::BranchPaused(BranchStatusData {
                status: Transition {
                    before: before.status,
                    after: after.status,
                },
                reason: Transition {
                    before: before.status_reason,
                    after: after.status_reason,
                },
            }),
        );
        publish_event_and_marker(root, transaction, &project, &event, None)
    })();
    settle_transaction_result(root, result)?;
    println!("Paused `{}`.", current_branch);
    Ok(())
}

fn cmd_abort(root: &Path, args: &AbortArgs) -> AppResult<()> {
    require_treework(root)?;
    let mut project = load_project(root)?;
    let mut branches = load_branches(root)?;
    let current_branch = resolve_mutation_target(root, &project, None)?;
    let branch_index = branches
        .iter()
        .position(|branch| branch.path == current_branch)
        .ok_or_else(|| AppError(format!("missing branch `{}`", current_branch)))?;
    let before = branches[branch_index].clone();
    if before.status == "complete" {
        return Err(AppError(format!(
            "cannot abort completed branch `{}`",
            before.path
        )));
    }
    if before.status == "aborted" {
        println!("Branch `{}` is already aborted.", current_branch);
        return Ok(());
    }
    let paths = branch_publication_paths(root, &current_branch, false);
    let transaction = PublicationTransaction::begin(root, "branch.abort", &paths, false)?;
    let timestamp = now();
    let branch = &mut branches[branch_index];
    branch.status = "aborted".to_string();
    branch.status_reason = args.reason.trim().to_string();
    branch.last_sync = timestamp.clone();
    let after = branch.clone();
    project.last_event_seq += 1;
    project.last_sync = timestamp.clone();
    let result = (|| {
        save_branches(root, &branches)?;
        sync_all_from_state(root, &project, &branches)?;
        inject_transaction_failure("transaction-after-accepted-state", &[])?;
        let event = EventEnvelope::new(
            project.last_event_seq,
            timestamp,
            &current_branch,
            args.reason.clone(),
            project.tree_revision,
            EventData::BranchAborted(BranchStatusData {
                status: Transition {
                    before: before.status,
                    after: after.status,
                },
                reason: Transition {
                    before: before.status_reason,
                    after: after.status_reason,
                },
            }),
        );
        publish_event_and_marker(root, transaction, &project, &event, None)
    })();
    settle_transaction_result(root, result)?;
    println!("Aborted `{}`.", current_branch);
    Ok(())
}

fn cmd_verify(root: &Path, args: &VerifyArgs) -> AppResult<()> {
    require_treework(root)?;
    let status = match args.result.as_str() {
        "passed" | "verified" => "verified",
        "failed" => "failed",
        "partial" => "partial",
        _ => "unverified",
    };
    let mut project = load_project(root)?;
    let target = resolve_mutation_target(root, &project, None)?;
    let mut branches = load_branches(root)?;
    let branch_index = branches
        .iter()
        .position(|branch| branch.path == target)
        .ok_or_else(|| AppError(format!("missing branch `{}`", target)))?;
    let before = branches[branch_index].clone();
    let verification_path = docs_dir_for_branch(root, &target).join("verification.md");
    if before.verification_status == status
        && verification_evidence_matches(&verification_path, &args.command, &args.result, &args.gap)
    {
        println!("Verification for `{}` is unchanged: {}.", target, status);
        return Ok(());
    }
    let paths = branch_publication_paths(root, &target, true);
    let transaction = PublicationTransaction::begin(root, "verification.record", &paths, false)?;
    let timestamp = now();
    let branch = &mut branches[branch_index];
    branch.verification_status = status.to_string();
    branch.last_sync = timestamp.clone();
    let after = branch.clone();
    project.last_event_seq += 1;
    project.last_sync = timestamp.clone();
    let result = (|| {
        save_branches(root, &branches)?;
        write_branch_verification_at(
            root,
            &target,
            &args.command,
            &args.result,
            &args.gap,
            &timestamp,
        )?;
        sync_all_from_state(root, &project, &branches)?;
        inject_transaction_failure("transaction-after-accepted-state", &[])?;
        let event = EventEnvelope::new(
            project.last_event_seq,
            timestamp,
            &target,
            format!("{} ({})", args.command, args.result),
            project.tree_revision,
            EventData::VerificationRecorded(VerificationRecordedData {
                verification: Transition {
                    before: before.verification_status,
                    after: after.verification_status,
                },
                evidence: VerificationEvidence {
                    command: args.command.clone(),
                    result: args.result.clone(),
                    gap: args.gap.clone(),
                },
            }),
        );
        publish_event_and_marker(root, transaction, &project, &event, None)
    })();
    settle_transaction_result(root, result)?;
    println!("Recorded verification for `{}`: {}.", target, status);
    Ok(())
}

fn cmd_complete(root: &Path, args: &CompleteArgs) -> AppResult<()> {
    require_treework(root)?;
    let mut project = load_project(root)?;
    let target = resolve_mutation_target(root, &project, None)?;
    let current_branches = load_branches(root)?;
    let current = current_branches
        .iter()
        .find(|branch| branch.path == target)
        .ok_or_else(|| AppError(format!("missing branch `{}`", target)))?;
    if current.status == "complete" {
        println!("Branch `{}` is already complete.", target);
        return Ok(());
    }
    let warnings = validate_completion(root, &target)?;
    if !warnings.is_empty() {
        println!("Cannot complete {}:", target);
        for warning in warnings {
            println!("- {}", warning);
        }
        return Err(AppError("completion gate failed".to_string()));
    }
    let mut branches = current_branches;
    let branch_index = branches
        .iter()
        .position(|branch| branch.path == target)
        .ok_or_else(|| AppError(format!("missing branch `{}`", target)))?;
    let before = branches[branch_index].clone();
    let cleanup =
        prepare_completion_cleanup(root, &mut branches[branch_index], args.keep_worktree)?;
    let publish_control_docs = cleanup.plan.is_some();
    let paths = branch_publication_paths(root, &target, false);
    let transaction = PublicationTransaction::begin(root, "branch.complete", &paths, false)?;
    let timestamp = now();
    let branch = &mut branches[branch_index];
    branch.status = "complete".to_string();
    branch.status_reason.clear();
    branch.last_sync = timestamp.clone();
    let after = branch.clone();
    project.last_event_seq += 1;
    project.last_sync = timestamp.clone();
    let result = (|| {
        save_branches(root, &branches)?;
        sync_all_from_state_with_control_branch(
            root,
            &project,
            &branches,
            publish_control_docs.then_some(target.as_str()),
        )?;
        inject_transaction_failure("transaction-after-accepted-state", &[])?;
        let event = EventEnvelope::new(
            project.last_event_seq,
            timestamp,
            &target,
            "Branch completed",
            project.tree_revision,
            EventData::BranchCompleted(BranchCompletedData {
                status: Transition {
                    before: before.status,
                    after: after.status,
                },
                reason: Transition {
                    before: before.status_reason,
                    after: after.status_reason,
                },
                verification: VerificationSummary {
                    status: after.verification_status,
                },
            }),
        );
        publish_event_and_marker(root, transaction, &project, &event, None)
    })();
    settle_transaction_result(root, result)?;
    println!("Completed `{}`.", target);
    if let Some(message) = cleanup.message {
        println!("{}", message);
    }
    if let Some(plan) = cleanup.plan {
        match execute_completion_cleanup(root, &plan) {
            Ok(message) => println!("{}", message),
            Err(error) => eprintln!(
                "TreeWork warning: branch `{}` is complete, but isolation cleanup did not finish: {}",
                target, error.0
            ),
        }
    }
    println!(
        "Merge reminder: review whether `{}` should be merged back to the main development branch.",
        target
    );
    Ok(())
}

struct CompletionCleanup {
    plan: Option<CompletionCleanupPlan>,
    message: Option<String>,
}

struct CompletionCleanupPlan {
    branch: String,
    workspace_path: PathBuf,
    keep_worktree: bool,
}

fn prepare_completion_cleanup(
    root: &Path,
    branch: &mut Branch,
    keep_worktree: bool,
) -> AppResult<CompletionCleanup> {
    if branch.isolation.mode != "git-worktree"
        || !branch.isolation.managed_by_treework
        || branch.isolation.workspace_path.trim().is_empty()
    {
        return Ok(CompletionCleanup {
            plan: None,
            message: None,
        });
    }

    let workspace_path = PathBuf::from(branch.isolation.workspace_path.trim());
    validate_normalized_absolute_path(&workspace_path, "managed worktree")?;
    if !workspace_path.exists() {
        branch.isolation.managed_by_treework = false;
        branch.isolation.last_status =
            "TreeWork management released; cleanup already satisfied because worktree is missing"
                .to_string();
        return Ok(CompletionCleanup {
            plan: None,
            message: Some(format!(
                "Isolation cleanup: managed worktree was already missing at `{}`.",
                workspace_path.display()
            )),
        });
    }

    let workspace_path = validate_managed_worktree(root, &branch.path, &workspace_path)?;
    if !git_worktree_clean(&workspace_path)? {
        return Err(AppError(format!(
            "managed worktree `{}` has uncommitted changes; commit, stash, or remove them before completing the branch",
            workspace_path.display()
        )));
    }

    if keep_worktree {
        branch.isolation.last_status =
            "TreeWork management released; cleanup intent: keep worktree and remove binding"
                .to_string();
    } else {
        branch.isolation.last_status =
            "TreeWork management released; cleanup intent: remove worktree".to_string();
    }
    branch.isolation.managed_by_treework = false;

    Ok(CompletionCleanup {
        plan: Some(CompletionCleanupPlan {
            branch: branch.path.clone(),
            workspace_path,
            keep_worktree,
        }),
        message: None,
    })
}

fn execute_completion_cleanup(root: &Path, plan: &CompletionCleanupPlan) -> AppResult<String> {
    let workspace_path = validate_managed_worktree(root, &plan.branch, &plan.workspace_path)?;
    if plan.keep_worktree {
        inject_transaction_failure("completion-cleanup-before-unbind", &[])?;
        remove_workspace_binding(&workspace_path)?;
        return Ok(format!(
            "Isolation cleanup: kept clean managed worktree `{}` and removed its TreeWork binding.",
            workspace_path.display()
        ));
    }

    inject_transaction_failure("completion-cleanup-before-remove", &[])?;
    let output = command_capture(
        root,
        "git",
        &[
            "worktree".to_string(),
            "remove".to_string(),
            workspace_path.display().to_string(),
        ],
    )?;
    if !output.success {
        return Err(AppError(format!(
            "failed to remove managed worktree `{}`: {}",
            workspace_path.display(),
            output.stderr_or_stdout()
        )));
    }
    Ok(format!(
        "Isolation cleanup: removed clean managed worktree `{}`.",
        workspace_path.display()
    ))
}

fn cmd_sync(root: &Path) -> AppResult<()> {
    require_treework(root)?;
    sync_all(root)?;
    println!("TreeWork generated views synchronized.");
    Ok(())
}

fn cmd_check(root: &Path, brief: bool) -> AppResult<()> {
    require_treework(root)?;
    let findings = validate_state(root)?;
    if brief {
        println!("TreeWork check: {} issue(s)", findings.len());
        for finding in findings.iter().take(5) {
            println!("- {}", finding);
        }
        return Ok(());
    }
    if findings.is_empty() {
        println!("TreeWork check passed.");
    } else {
        println!("TreeWork check found {} issue(s):", findings.len());
        for finding in findings {
            println!("- {}", finding);
        }
    }
    Ok(())
}

fn cmd_graph(root: &Path, command: GraphCommand) -> AppResult<()> {
    match command {
        GraphCommand::Render => {
            require_treework(root)?;
            render_graph(root)?;
            println!(
                "Rendered {}",
                tw_dir(root).join("out/project-map.html").display()
            );
        }
        GraphCommand::Serve(args) => serve_graph(root, &args)?,
    }
    Ok(())
}

fn serve_graph(root: &Path, args: &GraphServeArgs) -> AppResult<()> {
    {
        let _lock = acquire_lock(root)?;
        require_treework(root)?;
        render_graph(root)?;
    }
    project_map_server::serve(root, args.port, args.once)
}

fn scaffold_treework(root: &Path) -> AppResult<()> {
    fs::create_dir_all(tw_dir(root).join("state"))?;
    fs::create_dir_all(tw_dir(root).join("branches"))?;
    fs::create_dir_all(tw_dir(root).join("out"))?;
    write_template_if_missing(root, "PROJECT.md", "project.md")?;
    write_template_if_missing(root, "tree.yaml", "tree.yaml")?;
    write_alignment_templates(root)?;
    write_template_if_missing(root, "spec.md", "root_spec.md")?;
    write_template_if_missing(root, "task_plan.md", "root_task_plan.md")?;
    write_template_if_missing(root, "progress.md", "progress.md")?;
    write_template_if_missing(root, "findings.md", "root_findings.md")?;
    write_file_if_missing(&tw_dir(root).join("events.jsonl"), "")?;
    Ok(())
}

fn initial_root_branch(timestamp: &str) -> Branch {
    Branch {
        path: "root".to_string(),
        parent: String::new(),
        title: String::new(),
        purpose: String::new(),
        scope: BranchScope::default(),
        intake_rationale: String::new(),
        status: "in_progress".to_string(),
        verification_status: "unverified".to_string(),
        sync_status: "clean".to_string(),
        isolation: BranchIsolation::default(),
        status_reason: String::new(),
        last_sync: timestamp.to_string(),
    }
}

fn write_alignment_templates(root: &Path) -> AppResult<()> {
    for (target, template) in [
        ("requirements.md", "requirements.md"),
        ("assumptions.md", "assumptions.md"),
        ("references.md", "references.md"),
        ("idea_inbox.md", "idea_inbox.md"),
    ] {
        write_template_if_missing(root, target, template)?;
    }
    Ok(())
}

fn write_template_if_missing(root: &Path, target: &str, template: &str) -> AppResult<()> {
    let content = load_template(template)?.replace("<ISO-8601 timestamp>", &now());
    write_file_if_missing(&tw_dir(root).join(target), &content)
}

fn load_template(name: &str) -> AppResult<String> {
    if let Ok(plugin_root) = env::var("TREEWORK_PLUGIN_ROOT") {
        let path = Path::new(&plugin_root).join("templates").join(name);
        if path.exists() {
            return read_to_string(&path);
        }
    }
    let content = match name {
        "project.md" => include_str!("../../../skills/treework/templates/project.md"),
        "requirements.md" => {
            include_str!("../../../skills/treework/templates/requirements.md")
        }
        "assumptions.md" => {
            include_str!("../../../skills/treework/templates/assumptions.md")
        }
        "references.md" => {
            include_str!("../../../skills/treework/templates/references.md")
        }
        "idea_inbox.md" => {
            include_str!("../../../skills/treework/templates/idea_inbox.md")
        }
        "progress.md" => {
            include_str!("../../../skills/treework/templates/progress.md")
        }
        "root_task_plan.md" => {
            include_str!("../../../skills/treework/templates/root_task_plan.md")
        }
        "root_findings.md" => {
            include_str!("../../../skills/treework/templates/root_findings.md")
        }
        "root_spec.md" => {
            include_str!("../../../skills/treework/templates/root_spec.md")
        }
        "branch_spec.md" => {
            include_str!("../../../skills/treework/templates/branch_spec.md")
        }
        "branch_task_plan.md" => {
            include_str!("../../../skills/treework/templates/branch_task_plan.md")
        }
        "branch_progress.md" => {
            include_str!("../../../skills/treework/templates/branch_progress.md")
        }
        "branch_findings.md" => {
            include_str!("../../../skills/treework/templates/branch_findings.md")
        }
        "branch_verification.md" => {
            include_str!("../../../skills/treework/templates/branch_verification.md")
        }
        "tree.yaml" => {
            include_str!("../../../skills/treework/templates/tree.yaml")
        }
        _ => return Err(AppError(format!("unknown TreeWork template `{}`", name))),
    };
    Ok(content.to_string())
}

fn create_branch_docs(root: &Path, path: &str, parent: &str) -> AppResult<()> {
    let dir = branch_dir(root, path);
    fs::create_dir_all(&dir)?;
    let replacements = |template: &str| {
        template
            .replace("<branch path>", path)
            .replace("<parent branch>", parent)
    };
    write_file_if_missing(
        &dir.join("task_plan.md"),
        &replacements(&load_template("branch_task_plan.md")?),
    )?;
    write_file_if_missing(
        &dir.join("findings.md"),
        &replacements(&load_template("branch_findings.md")?),
    )?;
    write_file_if_missing(
        &dir.join("progress.md"),
        &replacements(&load_template("branch_progress.md")?),
    )?;
    write_file_if_missing(
        &dir.join("verification.md"),
        &replacements(&load_template("branch_verification.md")?),
    )?;
    Ok(())
}

fn sync_branch_plan_to_task_plan(
    root: &Path,
    branch: &Branch,
    changes: BranchPlanChanges,
) -> AppResult<()> {
    let path = branch_dir(root, &branch.path).join("task_plan.md");
    if !path.exists() {
        return Ok(());
    }
    let mut content = read_to_string(&path)?;
    if changes.title {
        content = upsert_markdown_header_field(&content, "Title", &branch.title);
    }
    if changes.rationale {
        content = upsert_markdown_list_field(
            &content,
            "Branch Intake Gate",
            "New branch rationale",
            &branch.intake_rationale,
        );
    }
    write_atomic(&path, &content)
}

fn upsert_markdown_header_field(content: &str, field: &str, value: &str) -> String {
    let prefix = format!("{}: ", field);
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    if let Some(index) = lines.iter().position(|line| line.starts_with(&prefix)) {
        if value.trim().is_empty() {
            lines.remove(index);
        } else {
            lines[index] = format!("{}: {}", field, value.trim());
        }
    } else if !value.trim().is_empty() {
        let insert_at = lines
            .iter()
            .position(|line| line.starts_with("Parent: "))
            .map(|index| index + 1)
            .unwrap_or(1);
        lines.insert(insert_at, format!("{}: {}", field, value.trim()));
    }
    let mut next = lines.join("\n");
    if content.ends_with('\n') {
        next.push('\n');
    }
    next
}

fn upsert_markdown_list_field(content: &str, section: &str, field: &str, value: &str) -> String {
    let marker = format!("## {}", section);
    let field_prefix = format!("- {}:", field);
    let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
    let Some(section_start) = lines.iter().position(|line| line.starts_with(&marker)) else {
        return content.to_string();
    };
    let section_end = lines
        .iter()
        .enumerate()
        .skip(section_start + 1)
        .find(|(_, line)| line.starts_with("## "))
        .map(|(index, _)| index)
        .unwrap_or(lines.len());
    if let Some(index) =
        (section_start + 1..section_end).find(|index| lines[*index].starts_with(&field_prefix))
    {
        lines[index] = format!("- {}: {}", field, value.trim());
    } else {
        lines.insert(section_end, format!("- {}: {}", field, value.trim()));
    }
    let mut next = lines.join("\n");
    if content.ends_with('\n') {
        next.push('\n');
    }
    next
}

fn write_branch_verification_at(
    root: &Path,
    branch: &str,
    command: &str,
    result: &str,
    gap: &str,
    recorded_at: &str,
) -> AppResult<()> {
    let content = format!(
        "# Verification\n\nBranch: {}\n\n## Latest Verification\n\n- Command: `{}`\n- Result: {}\n- Coverage gap: {}\n- Recorded: {}\n",
        branch, command, result, gap, recorded_at
    );
    write_atomic(
        &docs_dir_for_branch(root, branch).join("verification.md"),
        &content,
    )
}

fn verification_evidence_matches(path: &Path, command: &str, result: &str, gap: &str) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    content
        .lines()
        .any(|line| line == format!("- Command: `{}`", command))
        && content
            .lines()
            .any(|line| line == format!("- Result: {}", result))
        && content
            .lines()
            .any(|line| line == format!("- Coverage gap: {}", gap))
}

fn sync_all(root: &Path) -> AppResult<()> {
    let project = load_project(root)?;
    let branches = load_branches(root)?;
    sync_all_from_state(root, &project, &branches)
}

fn sync_all_from_state(root: &Path, project: &Project, branches: &[Branch]) -> AppResult<()> {
    sync_all_from_state_with_control_branch(root, project, branches, None)
}

fn sync_all_from_state_with_control_branch(
    root: &Path,
    project: &Project,
    branches: &[Branch],
    control_branch: Option<&str>,
) -> AppResult<()> {
    sync_root_progress(root, project, branches)?;
    for branch in branches {
        if branch.path != "root" {
            sync_branch_progress(root, branch, control_branch == Some(branch.path.as_str()))?;
        }
    }
    Ok(())
}

fn sync_root_progress(root: &Path, project: &Project, _branches: &[Branch]) -> AppResult<()> {
    let path = tw_dir(root).join("progress.md");
    let mut content = read_to_string(&path).unwrap_or_else(|_| "# Progress\n\n".to_string());
    let status_block = format!(
        "<!-- treework:root-status:start -->\nStage: {}\nLead cursor: {}\nAccepted tree revision: {}\nTree editing: {}\nLast sync: {}\n<!-- treework:root-status:end -->",
        project.stage,
        project.current_branch,
        project.tree_revision,
        if project.tree_editing.is_some() { "open" } else { "closed" },
        project.last_sync
    );
    content = replace_block(&content, "treework:root-status", &status_block);
    content = remove_managed_block(&content, "treework:branch-table");
    if !content.contains("## Global Reality") {
        content.push_str(
            "\n\n## Global Reality\n\nAgent writes global reality here.\n\n\
             ## Unverified Or Paused Work\n\nAgent records project-level gaps here.\n\n\
             ## Recent Branch Returns\n\nAgent records meaningful branch outcomes here.\n",
        );
    }
    write_atomic(&path, &content)
}

fn remove_managed_block(content: &str, key: &str) -> String {
    let start_marker = format!("<!-- {}:start -->", key);
    let end_marker = format!("<!-- {}:end -->", key);
    let (Some(start), Some(end)) = (content.find(&start_marker), content.find(&end_marker)) else {
        return content.to_string();
    };
    let end = end + end_marker.len();
    let mut next = format!("{}{}", &content[..start], &content[end..]);
    while next.contains("\n\n\n") {
        next = next.replace("\n\n\n", "\n\n");
    }
    next
}

fn sync_branch_progress(root: &Path, branch: &Branch, force_control: bool) -> AppResult<()> {
    let active_branch = invocation().branch.clone();
    let dir = if !force_control && active_branch.as_deref() == Some(branch.path.as_str()) {
        docs_dir_for_branch(root, &branch.path)
    } else {
        branch_dir(root, &branch.path)
    };
    let path = dir.join("progress.md");
    let mut content = read_to_string(&path)
        .unwrap_or_else(|_| format!("# Progress\n\nBranch: {}\n\n", branch.path));
    let block = format!(
        "<!-- treework:status:start -->\nBranch: {}\nParent: {}\nStatus: {}\nVerification: {}\nLast sync: {}\n<!-- treework:status:end -->",
        branch.path,
        branch.parent,
        branch.status,
        branch.verification_status,
        branch.last_sync
    );
    content = replace_block(&content, "treework:status", &block);
    write_atomic(&path, &content)
}

fn compute_graph_layout(
    branches: &[Branch],
    edge_count: usize,
) -> (HashMap<String, GraphNodeLayout>, GraphLayoutMeta) {
    let x_spacing = 4.0;
    let y_spacing = 3.0;
    let branch_ids: HashSet<String> = branches
        .iter()
        .filter(|branch| !branch.path.is_empty())
        .map(|branch| branch.path.clone())
        .collect();
    let root_id = if branch_ids.contains("root") {
        "root".to_string()
    } else {
        branches
            .iter()
            .find(|branch| !branch.path.is_empty())
            .map(|branch| branch.path.clone())
            .unwrap_or_else(|| "root".to_string())
    };

    let mut children: HashMap<String, Vec<String>> = HashMap::new();
    for branch in branches {
        if branch.path.is_empty() || branch.path == root_id {
            continue;
        }
        let parent = if branch.parent.is_empty()
            || branch.parent == branch.path
            || !branch_ids.contains(&branch.parent)
        {
            root_id.clone()
        } else {
            branch.parent.clone()
        };
        children
            .entry(parent)
            .or_default()
            .push(branch.path.clone());
    }
    for child_ids in children.values_mut() {
        let mut seen = HashSet::new();
        child_ids.retain(|id| seen.insert(id.clone()));
    }

    let mut layouts = HashMap::new();
    let mut visiting = HashSet::new();
    let mut next_order = 0;
    if branch_ids.contains(&root_id) {
        assign_tree_layout(
            &root_id,
            0,
            &children,
            &mut layouts,
            &mut visiting,
            &mut next_order,
            x_spacing,
            y_spacing,
        );
    }

    let mut remaining: Vec<String> = branches
        .iter()
        .filter(|branch| !branch.path.is_empty())
        .map(|branch| branch.path.clone())
        .collect();
    let mut seen = HashSet::new();
    remaining.retain(|id| seen.insert(id.clone()));
    for branch_id in remaining {
        if layouts.contains_key(&branch_id) {
            continue;
        }
        assign_tree_layout(
            &branch_id,
            1,
            &children,
            &mut layouts,
            &mut visiting,
            &mut next_order,
            x_spacing,
            y_spacing,
        );
    }

    let min_x = layouts
        .values()
        .map(|layout| layout.x)
        .fold(f64::INFINITY, f64::min);
    let max_x = layouts
        .values()
        .map(|layout| layout.x)
        .fold(f64::NEG_INFINITY, f64::max);
    if min_x.is_finite() && max_x.is_finite() {
        let center = (min_x + max_x) / 2.0;
        for layout in layouts.values_mut() {
            layout.x -= center;
        }
    }

    let max_depth = layouts
        .values()
        .map(|layout| layout.depth)
        .max()
        .unwrap_or(0);
    let meta = GraphLayoutMeta {
        algorithm: "treework_tidy_v1".to_string(),
        x_spacing,
        y_spacing,
        max_depth,
        node_count: layouts.len(),
        edge_count,
    };
    (layouts, meta)
}

#[allow(clippy::too_many_arguments)]
fn assign_tree_layout(
    branch_id: &str,
    depth: usize,
    children: &HashMap<String, Vec<String>>,
    layouts: &mut HashMap<String, GraphNodeLayout>,
    visiting: &mut HashSet<String>,
    next_order: &mut usize,
    x_spacing: f64,
    y_spacing: f64,
) -> (f64, usize, usize) {
    if let Some(existing) = layouts.get(branch_id) {
        return (existing.x, existing.order, existing.subtree_size);
    }
    if visiting.contains(branch_id) {
        let order = *next_order;
        *next_order += 1;
        return (order as f64 * x_spacing, order, 1);
    }

    visiting.insert(branch_id.to_string());
    let child_ids = children.get(branch_id).cloned().unwrap_or_default();
    let mut child_count = 0;
    let mut child_x_total = 0.0;
    let mut first_order = usize::MAX;
    let mut subtree_size = 1;

    for child_id in child_ids {
        if child_id == branch_id {
            continue;
        }
        let (child_x, child_order, child_subtree_size) = assign_tree_layout(
            &child_id,
            depth + 1,
            children,
            layouts,
            visiting,
            next_order,
            x_spacing,
            y_spacing,
        );
        child_count += 1;
        child_x_total += child_x;
        first_order = first_order.min(child_order);
        subtree_size += child_subtree_size;
    }

    let (x, order) = if child_count == 0 {
        let order = *next_order;
        *next_order += 1;
        (order as f64 * x_spacing, order)
    } else {
        (child_x_total / child_count as f64, first_order)
    };
    let layout = GraphNodeLayout {
        x,
        y: depth as f64 * y_spacing,
        depth,
        order,
        subtree_size,
    };
    visiting.remove(branch_id);
    layouts.insert(branch_id.to_string(), layout);
    (x, order, subtree_size)
}

fn default_graph_node_layout(order: usize) -> GraphNodeLayout {
    GraphNodeLayout {
        x: order as f64 * 4.0,
        y: 0.0,
        depth: 0,
        order,
        subtree_size: 1,
    }
}

fn render_graph(root: &Path) -> AppResult<()> {
    let out_dir = prepare_graph_output(root)?;
    let panel_html = copy_graph_panel_assets(root, &out_dir)?;
    let project = load_project(root)?;
    let branches = load_branches(root)?;
    let edges = load_edges(root)?;
    let (layout_by_id, layout_meta) = compute_graph_layout(&branches, edges.len());
    let projection = GraphProjection {
        meta: GraphProjectionMeta {
            current_branch: project.current_branch.clone(),
            stage: project.stage.clone(),
            tree_revision: project.tree_revision,
            last_event_seq: project.last_event_seq,
            last_sync: project.last_sync.clone(),
            layout: layout_meta,
        },
        nodes: branches
            .iter()
            .enumerate()
            .map(|(index, b)| GraphNode {
                id: b.path.clone(),
                parent: b.parent.clone(),
                title: b.title.clone(),
                purpose: b.purpose.clone(),
                accepts: b.scope.accepts.clone(),
                excludes: b.scope.excludes.clone(),
                intake_rationale: b.intake_rationale.clone(),
                status: b.status.clone(),
                verification: b.verification_status.clone(),
                sync_status: b.sync_status.clone(),
                status_reason: b.status_reason.clone(),
                last_sync: b.last_sync.clone(),
                layout: layout_by_id
                    .get(&b.path)
                    .cloned()
                    .unwrap_or_else(|| default_graph_node_layout(index)),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|e| GraphProjectionEdge {
                id: e.id.clone(),
                from: e.from.clone(),
                to: e.to.clone(),
                kind: e.kind.clone(),
                label: e.user_label.clone(),
            })
            .collect(),
    };
    let graph_json = serde_json::to_string_pretty(&projection)?;
    let mut graph_file = graph_json.clone();
    graph_file.push('\n');
    validate_graph_output(root, &out_dir)?;
    write_atomic(&out_dir.join("graph.json"), &graph_file)?;
    let html = panel_html.unwrap_or_else(|| {
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>TreeWork Project Map unavailable</title>
  </head>
  <body>
    <main>
      <h1>Project Map assets are unavailable</h1>
      <p>Run TreeWork through the packaged plugin so its frontend assets can be loaded.</p>
    </main>
  </body>
</html>
"#
        .to_string()
    });
    validate_graph_output(root, &out_dir)?;
    write_atomic(&out_dir.join("project-map.html"), &html)
}

fn prepare_graph_output(root: &Path) -> AppResult<PathBuf> {
    let treework = tw_dir(root);
    let treework_metadata = fs::symlink_metadata(&treework).map_err(|err| {
        AppError(format!(
            "Project Map output requires a controlled .TreeWork directory at {}: {}",
            treework.display(),
            err
        ))
    })?;
    if treework_metadata.file_type().is_symlink() || !treework_metadata.is_dir() {
        return Err(AppError(format!(
            "Project Map output refuses symlinked or non-directory .TreeWork path {}",
            treework.display()
        )));
    }

    let out_dir = treework.join("out");
    match fs::symlink_metadata(&out_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(AppError(format!(
                "Project Map output refuses symlinked or non-directory output path {}",
                out_dir.display()
            )));
        }
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::NotFound => fs::create_dir(&out_dir)?,
        Err(err) => return Err(err.into()),
    }
    validate_graph_output(root, &out_dir)?;
    Ok(out_dir)
}

fn validate_graph_output(root: &Path, out_dir: &Path) -> AppResult<()> {
    let treework = tw_dir(root);
    let treework_metadata = fs::symlink_metadata(&treework)?;
    if treework_metadata.file_type().is_symlink() || !treework_metadata.is_dir() {
        return Err(AppError(format!(
            "Project Map output refuses symlinked or non-directory .TreeWork path {}",
            treework.display()
        )));
    }
    let treework_canonical = fs::canonicalize(treework)?;
    let out_metadata = fs::symlink_metadata(out_dir)?;
    if out_metadata.file_type().is_symlink() || !out_metadata.is_dir() {
        return Err(AppError(format!(
            "Project Map output refuses symlinked or non-directory output path {}",
            out_dir.display()
        )));
    }
    let out_canonical = fs::canonicalize(out_dir)?;
    if out_canonical.parent() != Some(treework_canonical.as_path()) {
        return Err(AppError(format!(
            "Project Map output path {} is outside the controlled .TreeWork directory",
            out_dir.display()
        )));
    }

    for (name, expects_directory) in [
        ("graph.json", false),
        ("graph.tmp", false),
        ("project-map.html", false),
        ("project-map.tmp", false),
        ("app.js", false),
        ("styles.css", false),
        ("vendor", true),
    ] {
        let path = out_dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError(format!(
                    "Project Map output refuses symlinked output path {}",
                    path.display()
                )));
            }
            Ok(metadata) if expects_directory && !metadata.is_dir() => {
                return Err(AppError(format!(
                    "Project Map output requires directory path {}",
                    path.display()
                )));
            }
            Ok(metadata) if !expects_directory && !metadata.is_file() => {
                return Err(AppError(format!(
                    "Project Map output requires regular file path {}",
                    path.display()
                )));
            }
            Ok(_) => {}
            Err(err) if err.kind() == io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn copy_graph_panel_assets(root: &Path, out_dir: &Path) -> AppResult<Option<String>> {
    validate_graph_output(root, out_dir)?;
    let Ok(plugin_root) = env::var("TREEWORK_PLUGIN_ROOT") else {
        return Ok(None);
    };
    let panel_src = PathBuf::from(plugin_root).join("assets/graph-panel");
    if !panel_src.exists() {
        return Ok(None);
    }
    let index_path = panel_src.join("index.html");
    if !index_path.is_file() {
        return Ok(None);
    }
    let index_html = read_to_string(&index_path)?;
    for file_name in ["app.js", "styles.css"] {
        if !panel_src.join(file_name).is_file() {
            return Ok(None);
        }
    }
    let vendor_out = out_dir.join("vendor");
    if vendor_out.exists() {
        fs::remove_dir_all(&vendor_out)?;
    }
    for entry in fs::read_dir(&panel_src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        if file_name == "index.html" {
            continue;
        }
        let source = entry.path();
        let target = out_dir.join(file_name);
        if fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(AppError(format!(
                "Project Map output refuses symlinked output path {}",
                target.display()
            )));
        }
        if source.is_dir() {
            copy_dir_recursive(&source, &target)?;
        } else {
            fs::copy(&source, &target).map_err(|err| {
                AppError(format!(
                    "failed to copy graph panel asset {} to {}: {}",
                    source.display(),
                    target.display(),
                    err
                ))
            })?;
        }
    }
    Ok(Some(index_html))
}

fn validate_state(root: &Path) -> AppResult<Vec<String>> {
    let project = load_project(root)?;
    let branches = load_branches(root)?;
    let edges = load_edges(root)?;
    let mut findings = Vec::new();
    if project.stage == "build_tree" && project.tree_revision > 0 && project.tree_editing.is_none()
    {
        findings.push("stage is `build_tree` but no Tree Editing Session is open".to_string());
    }
    if project.stage != "build_tree" && project.tree_editing.is_some() {
        findings.push(format!(
            "Tree Editing Session is open while stage is `{}`",
            project.stage
        ));
    }
    if project.tree_revision > 0 {
        let accepted_path = accepted_tree_path(root);
        if !accepted_path.exists() {
            findings.push("accepted tree has no state/tree.json snapshot".to_string());
        } else {
            let accepted: AcceptedTreeState = read_json(&accepted_path)?;
            if accepted.revision != project.tree_revision {
                findings.push(format!(
                    "accepted Tree revision {} does not match project revision {}",
                    accepted.revision, project.tree_revision
                ));
            }
            if accepted.source_hash != project.tree_hash {
                findings.push("accepted Tree source hash does not match project state".to_string());
            }
            if accepted_tree_state_hash(&accepted)? != accepted.state_hash {
                findings.push("accepted Tree state hash is invalid".to_string());
            }
            let accepted_by_id: HashMap<&str, &AcceptedTreeNode> = accepted
                .nodes
                .iter()
                .map(|node| (node.id.as_str(), node))
                .collect();
            for branch in &branches {
                match accepted_by_id.get(branch.path.as_str()) {
                    Some(node)
                        if node.parent == branch.parent
                            && node.title == branch.title
                            && node.purpose == branch.purpose => {}
                    Some(_) => findings.push(format!(
                        "branch state for `{}` diverges from accepted Tree metadata",
                        branch.path
                    )),
                    None => findings.push(format!(
                        "branch state `{}` is absent from accepted Tree",
                        branch.path
                    )),
                }
            }
            for node in &accepted.nodes {
                if !branches.iter().any(|branch| branch.path == node.id) {
                    findings.push(format!(
                        "accepted Tree branch `{}` has no lifecycle state",
                        node.id
                    ));
                }
            }
            let mut expected_edges = HashSet::new();
            for node in &accepted.nodes {
                if !node.parent.is_empty() {
                    expected_edges.insert((
                        node.parent.clone(),
                        node.id.clone(),
                        "parent_of".to_string(),
                    ));
                }
                for dependency in &node.depends_on {
                    expected_edges.insert((
                        node.id.clone(),
                        dependency.clone(),
                        "depends_on".to_string(),
                    ));
                }
            }
            let actual_edges: HashSet<(String, String, String)> = edges
                .iter()
                .map(|edge| (edge.from.clone(), edge.to.clone(), edge.kind.clone()))
                .collect();
            for edge in expected_edges.difference(&actual_edges) {
                findings.push(format!(
                    "accepted Tree relation `{}` -> `{}` [{}] is missing from graph state",
                    edge.0, edge.1, edge.2
                ));
            }
            for edge in actual_edges.difference(&expected_edges) {
                findings.push(format!(
                    "graph state contains relation `{}` -> `{}` [{}] absent from accepted Tree",
                    edge.0, edge.1, edge.2
                ));
            }
        }
    }
    if project.tree_editing.is_none() && !project.tree_hash.is_empty() {
        let draft = read_to_string(&tree_document_path(root))?;
        if stable_hash_str(&draft) != project.tree_hash {
            findings.push(
                "tree.yaml changed outside a Tree Editing Session; run `tw tree update` before editing topology"
                    .to_string(),
            );
        }
    }
    if !branches.iter().any(|b| b.path == project.current_branch) {
        findings.push(format!(
            "current branch `{}` does not exist",
            project.current_branch
        ));
    }
    const BRANCH_STATUSES: [&str; 5] = ["pending", "in_progress", "paused", "complete", "aborted"];
    for branch in &branches {
        if !BRANCH_STATUSES.contains(&branch.status.as_str()) {
            findings.push(format!(
                "branch `{}` has unsupported status `{}`",
                branch.path, branch.status
            ));
        }
        if branch.status == "complete" && branch.verification_status != "verified" {
            findings.push(format!("complete branch `{}` is not verified", branch.path));
        }
        if branch.status == "aborted" && branch.status_reason.trim().is_empty() {
            findings.push(format!("aborted branch `{}` has no reason", branch.path));
        }
    }
    for edge in &edges {
        if !branches.iter().any(|b| b.path == edge.from) {
            findings.push(format!(
                "edge `{}` references missing from node `{}`",
                edge.id, edge.from
            ));
        }
        if !branches.iter().any(|b| b.path == edge.to) {
            findings.push(format!(
                "edge `{}` references missing to node `{}`",
                edge.id, edge.to
            ));
        }
    }
    Ok(findings)
}

fn validate_prepared_state(
    project: &Project,
    branches: &[Branch],
    edges: &[Edge],
    accepted: Option<&AcceptedTreeState>,
) -> AppResult<Vec<String>> {
    let mut findings = Vec::new();
    if project.stage == "build_tree" && project.tree_revision > 0 && project.tree_editing.is_none()
    {
        findings.push("stage is `build_tree` but no Tree Editing Session is open".to_string());
    }
    if project.stage != "build_tree" && project.tree_editing.is_some() {
        findings.push(format!(
            "Tree Editing Session is open while stage is `{}`",
            project.stage
        ));
    }
    if project.tree_revision == 0 {
        if accepted.is_some() {
            findings.push("revision-zero state unexpectedly contains an accepted Tree".to_string());
        }
    } else {
        let Some(accepted) = accepted else {
            findings.push("accepted tree has no state/tree.json snapshot".to_string());
            return Ok(findings);
        };
        if accepted.revision != project.tree_revision {
            findings.push(format!(
                "accepted Tree revision {} does not match project revision {}",
                accepted.revision, project.tree_revision
            ));
        }
        if accepted.source_hash != project.tree_hash {
            findings.push("accepted Tree source hash does not match project state".to_string());
        }
        if accepted_tree_state_hash(accepted)? != accepted.state_hash {
            findings.push("accepted Tree state hash is invalid".to_string());
        }
        let accepted_by_id: HashMap<&str, &AcceptedTreeNode> = accepted
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node))
            .collect();
        for branch in branches {
            match accepted_by_id.get(branch.path.as_str()) {
                Some(node)
                    if node.parent == branch.parent
                        && node.title == branch.title
                        && node.purpose == branch.purpose => {}
                Some(_) => findings.push(format!(
                    "branch state for `{}` diverges from accepted Tree metadata",
                    branch.path
                )),
                None => findings.push(format!(
                    "branch state `{}` is absent from accepted Tree",
                    branch.path
                )),
            }
        }
        let mut expected_edges = HashSet::new();
        for node in &accepted.nodes {
            if !node.parent.is_empty() {
                expected_edges.insert((
                    node.parent.clone(),
                    node.id.clone(),
                    "parent_of".to_string(),
                ));
            }
            for dependency in &node.depends_on {
                expected_edges.insert((
                    node.id.clone(),
                    dependency.clone(),
                    "depends_on".to_string(),
                ));
            }
        }
        let actual_edges: HashSet<(String, String, String)> = edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone(), edge.kind.clone()))
            .collect();
        if expected_edges != actual_edges {
            findings
                .push("prepared graph state does not match accepted Tree relations".to_string());
        }
    }
    if !branches
        .iter()
        .any(|branch| branch.path == project.current_branch)
    {
        findings.push(format!(
            "current branch `{}` does not exist",
            project.current_branch
        ));
    }
    Ok(findings)
}

fn validate_completion(root: &Path, branch_path: &str) -> AppResult<Vec<String>> {
    let branches = load_branches(root)?;
    let mut findings = Vec::new();
    let branch = branches
        .iter()
        .find(|b| b.path == branch_path)
        .ok_or_else(|| AppError(format!("missing branch `{}`", branch_path)))?;
    if branch.verification_status != "verified" {
        findings.push(format!(
            "verification is `{}`, not verified",
            branch.verification_status
        ));
    }
    if branch.status == "aborted" {
        findings.push(format!("branch is aborted: {}", branch.status_reason));
    }
    let task_plan = read_to_string(&docs_dir_for_branch(root, branch_path).join("task_plan.md"))
        .unwrap_or_default();
    if !acceptance_complete(&task_plan) {
        findings.push("acceptance checklist is missing or incomplete".to_string());
    }
    findings.extend(validate_state(root)?);
    Ok(findings)
}

fn acceptance_complete(markdown: &str) -> bool {
    let Some(start) = markdown.find("## Acceptance") else {
        return false;
    };
    let rest = &markdown[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    let section = &rest[..end];
    section.contains("- [x]") && !section.contains("- [ ]")
}

fn load_project(root: &Path) -> AppResult<Project> {
    let path = tw_dir(root).join("state/project.json");
    if !path.exists() {
        return Err(AppError(
            "TreeWork project state is missing; run `tw init`.".to_string(),
        ));
    }
    read_json(&path)
}

fn save_project(root: &Path, project: &Project) -> AppResult<()> {
    write_json_pretty(&tw_dir(root).join("state/project.json"), project)
}

fn load_branches(root: &Path) -> AppResult<Vec<Branch>> {
    let mut state: BranchState = read_json(&tw_dir(root).join("state/branches.json"))?;
    for branch in &mut state.branches {
        if branch.status == "blocked" {
            branch.status = "paused".to_string();
            if branch.status_reason.trim().is_empty() {
                branch.status_reason = "Migrated from legacy blocked state.".to_string();
            }
        } else if branch.status == "superseded" {
            branch.status = "aborted".to_string();
            if branch.status_reason.trim().is_empty() {
                branch.status_reason = "Migrated from legacy superseded state.".to_string();
            }
        }
    }
    Ok(state.branches)
}

fn save_branches(root: &Path, branches: &[Branch]) -> AppResult<()> {
    let state = BranchState {
        branches: branches.to_vec(),
    };
    write_json_pretty(&tw_dir(root).join("state/branches.json"), &state)
}

fn load_edges(root: &Path) -> AppResult<Vec<Edge>> {
    let state: GraphState = read_json(&tw_dir(root).join("state/graph.json"))?;
    Ok(state.edges)
}

fn save_edges(root: &Path, edges: &[Edge]) -> AppResult<()> {
    let state = GraphState {
        edges: edges.to_vec(),
    };
    write_json_pretty(&tw_dir(root).join("state/graph.json"), &state)
}

fn next_edge_id(edges: &[Edge]) -> String {
    let mut next = edges.len() + 1;
    let existing: HashSet<&str> = edges.iter().map(|edge| edge.id.as_str()).collect();
    loop {
        let candidate = format!("edge-{}", next);
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
        next += 1;
    }
}

fn publish_event_and_marker(
    root: &Path,
    mut transaction: PublicationTransaction,
    project: &Project,
    event: &EventEnvelope,
    checkpoint: Option<(&str, &str)>,
) -> AppResult<()> {
    append_typed_event(root, event)?;
    inject_transaction_failure(
        "transaction-after-event",
        if event.event_type == "tree.applied" {
            &["tree-apply-after-event"]
        } else {
            &[]
        },
    )?;
    transaction.prepare_intent(project, checkpoint)?;
    transaction.sync_before_marker()?;
    inject_transaction_failure("transaction-after-durable-intent", &[])?;
    save_project(root, project)?;
    transaction.sync_marker()?;
    inject_transaction_failure("transaction-after-project-marker", &[])?;
    transaction.finish()
}

fn append_typed_event(root: &Path, event: &EventEnvelope) -> AppResult<()> {
    let path = tw_dir(root).join("events.jsonl");
    let bytes = if path.exists() {
        fs::read(&path)?
    } else {
        Vec::new()
    };
    let events = event::parse_event_log(&bytes)
        .map_err(|error| AppError(format!("cannot append event: {}", error)))?;
    let tail = events.last().map(|item| item.seq()).unwrap_or(0);
    if tail + 1 != event.seq {
        return Err(AppError(format!(
            "cannot append event sequence {} after {}",
            event.seq, tail
        )));
    }
    let line = event.to_json_line().map_err(|error| {
        AppError(format!(
            "failed to serialize {} event: {}",
            event.event_type, error
        ))
    })?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

fn inject_transaction_failure(point: &str, aliases: &[&str]) -> AppResult<()> {
    let Ok(configured) = env::var("TREEWORK_TEST_FAILPOINT") else {
        return Ok(());
    };
    let matches = configured == point || aliases.iter().any(|alias| configured == *alias);
    let crash_matches = configured == format!("crash-{}", point)
        || aliases
            .iter()
            .any(|alias| configured == format!("crash-{}", alias));
    if crash_matches {
        std::process::exit(86);
    }
    if matches {
        return Err(AppError(format!(
            "injected transaction failure at {}",
            point
        )));
    }
    Ok(())
}

fn settle_transaction_result(root: &Path, result: AppResult<()>) -> AppResult<()> {
    let Err(error) = result else {
        return Ok(());
    };
    match recover_pending_transaction(root) {
        Ok(RecoveryOutcome::FinishedForward) => Ok(()),
        Ok(_) => Err(error),
        Err(recovery_error) => Err(AppError(format!(
            "{}; transaction recovery also failed: {}",
            error.0, recovery_error.0
        ))),
    }
}

fn discover_invocation(cwd: &Path) -> AppResult<Invocation> {
    let workspace_root = discover_workspace_root(cwd)?;
    let common_descriptor = load_control_descriptor(&workspace_root)?;
    let control_root = match &common_descriptor {
        Some(descriptor) => canonical_existing(
            Path::new(&descriptor.control_root),
            "Git common TreeWork control descriptor",
        )?,
        None => workspace_root.clone(),
    };

    if let Some(descriptor) = &common_descriptor {
        let expected = project_id_for_root(&control_root);
        if descriptor.project_id != expected {
            return Err(AppError(format!(
                "TreeWork context error: control descriptor project `{}` does not match `{}`",
                descriptor.project_id, expected
            )));
        }
    }

    let binding = load_worktree_branch_binding(&workspace_root)?;
    if let Some(binding) = &binding {
        validate_binding_shape(binding)?;
        let expected_workspace =
            canonical_existing(Path::new(&binding.workspace), "TreeWork branch workspace")?;
        if expected_workspace != workspace_root {
            return Err(AppError(format!(
                "TreeWork workspace error: invocation workspace `{}` does not match bound workspace `{}`",
                workspace_root.display(),
                expected_workspace.display()
            )));
        }
        let expected_project = project_id_for_root(&control_root);
        if binding.project_id != expected_project {
            return Err(AppError(format!(
                "TreeWork workspace error: branch binding project `{}` does not match `{}`",
                binding.project_id, expected_project
            )));
        }
    }

    Ok(Invocation {
        control_root,
        workspace_root,
        branch: binding.map(|binding| binding.branch),
    })
}

fn discover_workspace_root(cwd: &Path) -> AppResult<PathBuf> {
    if let Some(root) = git_path(cwd, "--show-toplevel") {
        return canonical_existing(&root, "Git worktree root");
    }
    for ancestor in cwd.ancestors() {
        if ancestor.join(TW_DIR).exists() {
            return canonical_existing(ancestor, "TreeWork workspace root");
        }
    }
    canonical_existing(cwd, "current workspace")
}

fn validate_binding_shape(binding: &WorktreeBranchBinding) -> AppResult<()> {
    if binding.version != 1 {
        return Err(AppError(format!(
            "TreeWork workspace error: unsupported branch binding version {}",
            binding.version
        )));
    }
    if binding.project_id.trim().is_empty()
        || binding.branch.trim().is_empty()
        || binding.workspace.trim().is_empty()
    {
        return Err(AppError(
            "TreeWork workspace error: project_id, branch, and workspace are required".to_string(),
        ));
    }
    Ok(())
}

fn load_control_descriptor(workspace_root: &Path) -> AppResult<Option<ControlRootDescriptor>> {
    let Some(common_dir) = git_path(workspace_root, "--git-common-dir") else {
        return Ok(None);
    };
    let path = common_dir.join(CONTROL_DESCRIPTOR);
    if !path.exists() {
        return Ok(None);
    }
    read_json(&path).map(Some).map_err(|err| {
        AppError(format!(
            "failed to read TreeWork control descriptor {}: {}",
            path.display(),
            err.0
        ))
    })
}

fn ensure_control_descriptor(control_root: &Path) -> AppResult<()> {
    let Some(common_dir) = git_path(control_root, "--git-common-dir") else {
        return Ok(());
    };
    let path = common_dir.join(CONTROL_DESCRIPTOR);
    let expected = ControlRootDescriptor {
        version: 1,
        project_id: project_id_for_root(control_root),
        control_root: canonical_existing(control_root, "TreeWork control root")?
            .display()
            .to_string(),
    };
    if path.exists() {
        let current: ControlRootDescriptor = read_json(&path)?;
        if current.project_id != expected.project_id
            || canonical_existing(Path::new(&current.control_root), "TreeWork control root")?
                != Path::new(&expected.control_root)
        {
            return Err(AppError(format!(
                "TreeWork context error: refusing to replace conflicting control descriptor {}",
                path.display()
            )));
        }
        return Ok(());
    }
    write_json_pretty(&path, &expected)
}

fn load_worktree_branch_binding(workspace_root: &Path) -> AppResult<Option<WorktreeBranchBinding>> {
    let Some(git_dir) = git_path(workspace_root, "--git-dir") else {
        return Ok(None);
    };
    let path = git_dir.join(WORKTREE_BRANCH_DESCRIPTOR);
    if !path.exists() {
        return Ok(None);
    }
    read_json(&path).map(Some).map_err(|err| {
        AppError(format!(
            "failed to read TreeWork branch binding {}: {}",
            path.display(),
            err.0
        ))
    })
}

fn validate_normalized_absolute_path(path: &Path, label: &str) -> AppResult<()> {
    if !path.is_absolute()
        || path.components().any(|component| {
            !matches!(
                component,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )
        })
    {
        return Err(AppError(format!(
            "{} path `{}` must be normalized and absolute",
            label,
            path.display()
        )));
    }
    Ok(())
}

fn validate_managed_worktree(
    control_root: &Path,
    branch: &str,
    workspace_path: &Path,
) -> AppResult<PathBuf> {
    validate_normalized_absolute_path(workspace_path, "managed worktree")?;
    let control_root = canonical_existing(control_root, "TreeWork control root")?;
    let workspace = canonical_existing(workspace_path, "managed worktree")?;
    let git_root = git_path(&workspace, "--show-toplevel")
        .ok_or_else(|| {
            AppError(format!(
                "managed worktree `{}` is not a Git worktree",
                workspace.display()
            ))
        })
        .and_then(|path| canonical_existing(&path, "managed Git worktree root"))?;
    if git_root != workspace {
        return Err(AppError(format!(
            "managed worktree path `{}` does not name its Git worktree root `{}`",
            workspace.display(),
            git_root.display()
        )));
    }

    let control_common = git_path(&control_root, "--git-common-dir")
        .ok_or_else(|| AppError("TreeWork control root is not a Git worktree".to_string()))
        .and_then(|path| canonical_existing(&path, "control Git common directory"))?;
    let workspace_common = git_path(&workspace, "--git-common-dir")
        .ok_or_else(|| {
            AppError(format!(
                "managed worktree `{}` has no Git common directory",
                workspace.display()
            ))
        })
        .and_then(|path| canonical_existing(&path, "managed Git common directory"))?;
    if workspace_common != control_common {
        return Err(AppError(format!(
            "worktree `{}` is not managed by control root `{}`",
            workspace.display(),
            control_root.display()
        )));
    }

    let descriptor = load_control_descriptor(&workspace)?.ok_or_else(|| {
        AppError(format!(
            "worktree `{}` has no TreeWork control descriptor",
            workspace.display()
        ))
    })?;
    let descriptor_root = canonical_existing(
        Path::new(&descriptor.control_root),
        "TreeWork control descriptor",
    )?;
    let expected_project = project_id_for_root(&control_root);
    if descriptor_root != control_root || descriptor.project_id != expected_project {
        return Err(AppError(format!(
            "worktree `{}` does not belong to TreeWork project `{}`",
            workspace.display(),
            expected_project
        )));
    }

    let binding = load_worktree_branch_binding(&workspace)?.ok_or_else(|| {
        AppError(format!(
            "worktree `{}` has no TreeWork branch binding",
            workspace.display()
        ))
    })?;
    validate_binding_shape(&binding)?;
    let bound_workspace =
        canonical_existing(Path::new(&binding.workspace), "TreeWork branch workspace")?;
    if binding.project_id != expected_project
        || binding.branch != branch
        || bound_workspace != workspace
    {
        return Err(AppError(format!(
            "worktree `{}` binding does not match project `{}` branch `{}`",
            workspace.display(),
            expected_project,
            branch
        )));
    }
    Ok(workspace)
}

pub(crate) fn validate_transaction_external_path(
    control_root: &Path,
    path: &Path,
) -> AppResult<()> {
    validate_normalized_absolute_path(path, "external transaction")?;
    let treework = path
        .ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == TW_DIR))
        .ok_or_else(|| {
            AppError(format!(
                "external transaction path `{}` is outside a TreeWork workspace",
                path.display()
            ))
        })?;
    let workspace_path = treework.parent().ok_or_else(|| {
        AppError(format!(
            "external transaction path `{}` has no workspace root",
            path.display()
        ))
    })?;
    let binding = load_worktree_branch_binding(workspace_path)?.ok_or_else(|| {
        AppError(format!(
            "external transaction path `{}` is not in a bound worktree",
            path.display()
        ))
    })?;
    let workspace = validate_managed_worktree(control_root, &binding.branch, workspace_path)?;
    let branch_docs = tw_dir(&workspace).join("branches").join(&binding.branch);
    let allowed = [
        branch_docs.join("progress.md"),
        branch_docs.join("verification.md"),
    ];
    if !allowed.iter().any(|candidate| candidate == path) {
        return Err(AppError(format!(
            "external transaction path `{}` is not a managed branch publication document",
            path.display()
        )));
    }
    let parent = path.parent().ok_or_else(|| {
        AppError(format!(
            "external transaction path `{}` has no parent directory",
            path.display()
        ))
    })?;
    if canonical_existing(parent, "external transaction document directory")? != parent {
        return Err(AppError(format!(
            "external transaction path `{}` crosses a symlinked directory",
            path.display()
        )));
    }
    Ok(())
}

fn write_workspace_binding(
    control_root: &Path,
    workspace_root: &Path,
    branch: &str,
) -> AppResult<WorktreeBranchBinding> {
    ensure_control_descriptor(control_root)?;
    let workspace = canonical_existing(workspace_root, "TreeWork branch workspace")?;
    let git_dir = git_path(&workspace, "--git-dir").ok_or_else(|| {
        AppError(format!(
            "cannot bind TreeWork context: `{}` is not a Git worktree",
            workspace.display()
        ))
    })?;
    let binding = WorktreeBranchBinding {
        version: 1,
        project_id: project_id_for_root(control_root),
        branch: branch.to_string(),
        workspace: workspace.display().to_string(),
    };
    write_json_pretty(&git_dir.join(WORKTREE_BRANCH_DESCRIPTOR), &binding)?;
    Ok(binding)
}

fn remove_workspace_binding(workspace_root: &Path) -> AppResult<()> {
    let Some(git_dir) = git_path(workspace_root, "--git-dir") else {
        return Ok(());
    };
    for name in [WORKTREE_BRANCH_DESCRIPTOR, "treework-context.json"] {
        let path = git_dir.join(name);
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn git_path(root: &Path, arg: &str) -> Option<PathBuf> {
    let value = command_stdout(root, "git", &["rev-parse", "--path-format=absolute", arg])?;
    let path = PathBuf::from(value);
    Some(if path.is_absolute() {
        path
    } else {
        root.join(path)
    })
}

fn canonical_existing(path: &Path, label: &str) -> AppResult<PathBuf> {
    fs::canonicalize(path).map_err(|err| {
        AppError(format!(
            "TreeWork context error: cannot resolve {} `{}`: {}",
            label,
            path.display(),
            err
        ))
    })
}

fn project_id_for_root(root: &Path) -> String {
    let path = root.to_string_lossy();
    let mut hash = 1469598103934665603_u64;
    for byte in path.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("project");
    format!("{}-{:016x}", branch_slug(name), hash)
}

fn invocation() -> &'static Invocation {
    INVOCATION
        .get()
        .expect("TreeWork invocation must be initialized before command dispatch")
}

fn require_control_workspace(action: &str) -> AppResult<()> {
    if !invocation().is_control_workspace() {
        return Err(AppError(format!(
            "TreeWork workspace error: `{}` is a branch worktree and may not {}; run this from the control workspace",
            invocation().workspace_root.display(),
            action
        )));
    }
    Ok(())
}

fn resolve_mutation_target(
    _root: &Path,
    project: &Project,
    explicit: Option<&str>,
) -> AppResult<String> {
    let Some(bound_branch) = &invocation().branch else {
        if !invocation().is_control_workspace() {
            return Err(AppError(format!(
                "TreeWork workspace error: linked worktree `{}` has no branch binding; refusing to use the control workspace cursor",
                invocation().workspace_root.display()
            )));
        }
        return Ok(explicit.unwrap_or(&project.current_branch).to_string());
    };
    if let Some(target) = explicit {
        if target != bound_branch {
            return Err(AppError(format!(
                "TreeWork workspace error: this worktree is bound to `{}`, but the command targeted `{}`. No state was changed",
                bound_branch, target
            )));
        }
    }
    Ok(bound_branch.clone())
}

fn detect_workspace_git(root: &Path) -> BranchWorkspaceGit {
    let git_root = command_stdout(root, "git", &["rev-parse", "--show-toplevel"]);
    let Some(git_root) = git_root else {
        return BranchWorkspaceGit {
            available: false,
            root: String::new(),
            current_branch: String::new(),
            head: String::new(),
            worktree_supported: false,
        };
    };
    let current_branch =
        command_stdout(root, "git", &["branch", "--show-current"]).unwrap_or_default();
    let head = command_stdout(root, "git", &["rev-parse", "--short", "HEAD"]).unwrap_or_default();
    let worktree_supported = command_stdout(root, "git", &["worktree", "list", "--porcelain"])
        .map(|_| true)
        .unwrap_or(false);
    BranchWorkspaceGit {
        available: true,
        root: git_root,
        current_branch,
        head,
        worktree_supported,
    }
}

fn command_stdout(root: &Path, program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

struct CommandCapture {
    success: bool,
    stdout: String,
    stderr: String,
}

impl CommandCapture {
    fn stderr_or_stdout(&self) -> String {
        if !self.stderr.trim().is_empty() {
            self.stderr.trim().to_string()
        } else {
            self.stdout.trim().to_string()
        }
    }
}

fn command_capture(root: &Path, program: &str, args: &[String]) -> AppResult<CommandCapture> {
    let output = Command::new(program)
        .args(args)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;
    Ok(CommandCapture {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
    })
}

fn git_ref_exists(root: &Path, ref_name: &str) -> bool {
    command_capture(
        root,
        "git",
        &[
            "rev-parse".to_string(),
            "--verify".to_string(),
            "--quiet".to_string(),
            ref_name.to_string(),
        ],
    )
    .map(|output| output.success)
    .unwrap_or(false)
}

fn git_worktree_clean(worktree_path: &Path) -> AppResult<bool> {
    let output = command_capture(
        worktree_path,
        "git",
        &["status".to_string(), "--porcelain".to_string()],
    )?;
    if !output.success {
        return Err(AppError(format!(
            "failed to inspect worktree `{}`: {}",
            worktree_path.display(),
            output.stderr_or_stdout()
        )));
    }
    Ok(output.stdout.trim().is_empty())
}

fn default_branch_worktree_path(root: &Path, branch: &str) -> PathBuf {
    let project_name = root
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("workspace");
    let base = root
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf());
    base.join(".treework-worktrees")
        .join(project_name)
        .join(branch_slug(branch))
}

fn default_branch_git_branch(branch: &str) -> String {
    format!("treework/{}", branch_slug(branch))
}

fn build_branch_recall(
    root: &Path,
    project: &Project,
    branch_path: &str,
) -> AppResult<BranchRecall> {
    let branches = load_branches(root)?;
    let branch = find_branch(&branches, branch_path)?.clone();
    let parent = if branch.parent.is_empty() {
        None
    } else {
        branches
            .iter()
            .find(|item| item.path == branch.parent)
            .cloned()
    };
    let mut children: Vec<Branch> = branches
        .iter()
        .filter(|item| item.parent == branch_path)
        .cloned()
        .collect();
    children.sort_by(|a, b| a.path.cmp(&b.path));

    let mut related_edges: Vec<Edge> = load_edges(root)?
        .into_iter()
        .filter(|edge| edge.from == branch_path || edge.to == branch_path)
        .collect();
    related_edges.sort_by(|a, b| a.id.cmp(&b.id));

    let docs = read_branch_docs(root, branch_path);
    let verification = branch_verification_summary(&branch, &docs);
    let (allowed_actions, blocked_actions) =
        branch_action_eligibility(root, project, &branch, &docs);
    Ok(BranchRecall {
        schema_version: "0.2".to_string(),
        generated_at: now(),
        tree_revision: project.tree_revision,
        publication_marker: RecallPublicationMarker {
            last_event_seq: project.last_event_seq,
            tree_revision: project.tree_revision,
            tree_hash: project.tree_hash.clone(),
        },
        project: RecallProject {
            stage: project.stage.clone(),
            current_branch: project.current_branch.clone(),
        },
        isolation: branch_isolation_summary(&branch),
        branch,
        parent,
        children,
        related_edges,
        docs,
        verification,
        allowed_actions,
        blocked_actions,
    })
}

fn branch_action_eligibility(
    root: &Path,
    project: &Project,
    branch: &Branch,
    docs: &BranchDocs,
) -> (Vec<String>, Vec<RecallBlockedAction>) {
    let mut allowed = Vec::new();
    let mut blocked = Vec::new();

    let mut enter = enter_target_blockers(&branch.path);
    match branch.status.as_str() {
        "complete" => enter.push(recall_blocker(
            "terminal_complete",
            "completed branches cannot be entered",
        )),
        "aborted" => enter.push(recall_blocker(
            "terminal_aborted",
            "aborted branches cannot be entered",
        )),
        _ => {}
    }
    record_action_eligibility("enter", enter, &mut allowed, &mut blocked);

    let mutation_target = mutation_target_blockers(project, &branch.path);

    let mut pause = clone_blockers(&mutation_target);
    match branch.status.as_str() {
        "complete" => pause.push(recall_blocker(
            "terminal_complete",
            "completed branches cannot be paused",
        )),
        "aborted" => pause.push(recall_blocker(
            "terminal_aborted",
            "aborted branches cannot be paused",
        )),
        _ => {}
    }
    record_action_eligibility("pause", pause, &mut allowed, &mut blocked);

    let mut abort = clone_blockers(&mutation_target);
    match branch.status.as_str() {
        "complete" => abort.push(recall_blocker(
            "terminal_complete",
            "completed branches cannot be aborted",
        )),
        "aborted" => abort.push(recall_blocker(
            "already_aborted",
            "the branch is already aborted",
        )),
        _ => {}
    }
    record_action_eligibility("abort", abort, &mut allowed, &mut blocked);

    let mut complete = mutation_target;
    match branch.status.as_str() {
        "complete" => complete.push(recall_blocker(
            "already_complete",
            "the branch is already complete",
        )),
        "aborted" => complete.push(recall_blocker(
            "terminal_aborted",
            "aborted branches cannot be completed",
        )),
        _ => {}
    }
    if !acceptance_complete(&docs.task_plan) {
        complete.push(recall_blocker(
            "acceptance_incomplete",
            "the Acceptance checklist is missing or incomplete",
        ));
    }
    if branch.verification_status != "verified" {
        complete.push(recall_blocker(
            "verification_not_verified",
            format!(
                "verification status is `{}` rather than `verified`",
                branch.verification_status
            ),
        ));
    }
    match validate_state(root) {
        Ok(findings) => {
            for finding in findings {
                complete.push(recall_blocker("state_inconsistent", finding));
            }
        }
        Err(error) => complete.push(recall_blocker(
            "state_validation_failed",
            format!("state validation could not finish: {}", error.0),
        )),
    }
    if branch.status != "complete" && branch.status != "aborted" {
        let mut cleanup_candidate = branch.clone();
        if let Err(error) = prepare_completion_cleanup(root, &mut cleanup_candidate, false) {
            complete.push(recall_blocker("workspace_unsafe", error.0));
        }
    }
    record_action_eligibility("complete", complete, &mut allowed, &mut blocked);

    (allowed, blocked)
}

fn enter_target_blockers(branch: &str) -> Vec<RecallActionBlocker> {
    match &invocation().branch {
        Some(bound) if bound != branch => vec![recall_blocker(
            "workspace_branch_mismatch",
            format!(
                "this worktree is bound to `{}` and cannot enter `{}`",
                bound, branch
            ),
        )],
        None if !invocation().is_control_workspace() => vec![recall_blocker(
            "workspace_binding_missing",
            "the linked worktree has no TreeWork branch binding",
        )],
        _ => Vec::new(),
    }
}

fn mutation_target_blockers(project: &Project, branch: &str) -> Vec<RecallActionBlocker> {
    match &invocation().branch {
        Some(bound) if bound != branch => vec![recall_blocker(
            "workspace_branch_mismatch",
            format!(
                "this worktree is bound to `{}`; branch-scoped mutations cannot target `{}`",
                bound, branch
            ),
        )],
        Some(_) => Vec::new(),
        None if !invocation().is_control_workspace() => vec![recall_blocker(
            "workspace_binding_missing",
            "the linked worktree has no TreeWork branch binding",
        )],
        None if project.current_branch != branch => vec![recall_blocker(
            "not_lead_cursor",
            format!(
                "the control-workspace cursor currently targets `{}`",
                project.current_branch
            ),
        )],
        None => Vec::new(),
    }
}

fn recall_blocker(code: impl Into<String>, reason: impl Into<String>) -> RecallActionBlocker {
    RecallActionBlocker {
        code: code.into(),
        reason: reason.into(),
    }
}

fn clone_blockers(blockers: &[RecallActionBlocker]) -> Vec<RecallActionBlocker> {
    blockers
        .iter()
        .map(|blocker| recall_blocker(&blocker.code, &blocker.reason))
        .collect()
}

fn record_action_eligibility(
    action: &str,
    blockers: Vec<RecallActionBlocker>,
    allowed: &mut Vec<String>,
    blocked: &mut Vec<RecallBlockedAction>,
) {
    if blockers.is_empty() {
        allowed.push(action.to_string());
        return;
    }
    blocked.push(RecallBlockedAction {
        action: action.to_string(),
        reason_codes: blockers.iter().map(|item| item.code.clone()).collect(),
        reasons: blockers.into_iter().map(|item| item.reason).collect(),
    });
}

fn branch_isolation_summary(branch: &Branch) -> BranchIsolationSummary {
    let workspace_path = branch.isolation.workspace_path.clone();
    let exists = if workspace_path.trim().is_empty() {
        false
    } else {
        PathBuf::from(&workspace_path).exists()
    };
    let clean = if exists && branch.isolation.mode == "git-worktree" {
        git_worktree_clean(&PathBuf::from(&workspace_path)).ok()
    } else {
        None
    };
    BranchIsolationSummary {
        mode: branch.isolation.mode.clone(),
        workspace_path,
        git_branch: branch.isolation.git_branch.clone(),
        managed_by_treework: branch.isolation.managed_by_treework,
        exists,
        clean,
        last_status: branch.isolation.last_status.clone(),
    }
}

fn render_branch_recall_markdown(recall: &BranchRecall, brief: bool) -> String {
    let mut content = format!(
        "# TreeWork Recall: {}\n\nGenerated: {}\n\n",
        recall.branch.path, recall.generated_at
    );
    content.push_str("## Status\n\n");
    content.push_str("| Field | Value |\n|---|---|\n");
    content.push_str(&format!(
        "| Stage | {} |\n| Lead cursor | {} |\n| Tree revision | {} |\n| Event sequence | {} |\n| Branch status | {} |\n| Verification | {} |\n| Acceptance complete | {} |\n| Parent | {} |\n| Children | {} |\n| Related edges | {} |\n",
        table_cell(&recall.project.stage),
        table_cell(&recall.project.current_branch),
        recall.tree_revision,
        recall.publication_marker.last_event_seq,
        table_cell(&recall.branch.status),
        table_cell(&recall.branch.verification_status),
        recall.verification.acceptance_complete,
        table_cell(if recall.branch.parent.is_empty() { "-" } else { &recall.branch.parent }),
        recall.children.len(),
        recall.related_edges.len(),
    ));

    content.push_str("\n## Action Eligibility\n\n");
    if recall.allowed_actions.is_empty() {
        content.push_str("- Allowed: none\n");
    } else {
        content.push_str(&format!(
            "- Allowed: {}\n",
            recall
                .allowed_actions
                .iter()
                .map(|action| format!("`{}`", action))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for action in &recall.blocked_actions {
        content.push_str(&format!("- `{}`: blocked\n", action.action));
        for (code, reason) in action.reason_codes.iter().zip(&action.reasons) {
            content.push_str(&format!("  - [{}] {}\n", code, reason));
        }
    }
    content.push_str(&format!(
        "\nEligibility was derived from committed revision {} at event {}. Commands revalidate before publishing.\n",
        recall.publication_marker.tree_revision, recall.publication_marker.last_event_seq
    ));

    content.push_str("\n## Isolation\n\n");
    if recall.isolation.mode.trim().is_empty() {
        content.push_str("- Mode: none\n");
    } else {
        content.push_str(&format!(
            "- Mode: `{}`\n- Workspace: `{}`\n- Git branch: `{}`\n- Managed by TreeWork: {}\n- Exists: {}\n- Clean: {}\n- Last status: {}\n",
            recall.isolation.mode,
            if recall.isolation.workspace_path.trim().is_empty() {
                "-"
            } else {
                recall.isolation.workspace_path.trim()
            },
            if recall.isolation.git_branch.trim().is_empty() {
                "-"
            } else {
                recall.isolation.git_branch.trim()
            },
            recall.isolation.managed_by_treework,
            recall.isolation.exists,
            recall
                .isolation
                .clean
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-".to_string()),
            if recall.isolation.last_status.trim().is_empty() {
                "-"
            } else {
                recall.isolation.last_status.trim()
            }
        ));
    }

    content.push_str("\n## Local Map\n\n");
    if let Some(parent) = &recall.parent {
        content.push_str(&format!(
            "- Parent: `{}` [{} / {}]\n",
            parent.path, parent.status, parent.verification_status
        ));
    } else {
        content.push_str("- Parent: none\n");
    }
    if recall.children.is_empty() {
        content.push_str("- Children: none\n");
    } else {
        content.push_str("- Children:\n");
        for child in &recall.children {
            content.push_str(&format!(
                "  - `{}` [{} / {}]\n",
                child.path, child.status, child.verification_status
            ));
        }
    }

    content.push_str("\n## Related Edges\n\n");
    if recall.related_edges.is_empty() {
        content.push_str("- None\n");
    } else {
        for edge in &recall.related_edges {
            content.push_str(&format!(
                "- `{}` -> `{}` [{}]: {}\n",
                edge.from, edge.to, edge.kind, edge.user_label
            ));
        }
    }

    if brief {
        return content;
    }

    content.push_str("\n## Branch Documents\n\n");
    push_doc_section(&mut content, "spec.md", &recall.docs.spec);
    push_doc_section(&mut content, "task_plan.md", &recall.docs.task_plan);
    push_doc_section(&mut content, "progress.md", &recall.docs.progress);
    push_doc_section(&mut content, "findings.md", &recall.docs.findings);
    push_doc_section(&mut content, "verification.md", &recall.docs.verification);
    content
}

fn branch_verification_summary(branch: &Branch, docs: &BranchDocs) -> BranchVerification {
    BranchVerification {
        status: branch.verification_status.clone(),
        acceptance_complete: acceptance_complete(&docs.task_plan),
        verification_doc_present: !docs.verification.trim().is_empty()
            && !docs.verification.contains("No verification recorded yet."),
        coverage_gap: extract_coverage_gap(&docs.verification),
    }
}

fn extract_coverage_gap(markdown: &str) -> String {
    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(value) = trimmed.strip_prefix("- Coverage gap:") {
            return value.trim().to_string();
        }
    }
    String::new()
}

fn read_branch_docs(root: &Path, branch_path: &str) -> BranchDocs {
    let dir = read_docs_dir_for_branch(root, branch_path);
    BranchDocs {
        spec: read_to_string(&dir.join("spec.md")).unwrap_or_default(),
        task_plan: read_to_string(&dir.join("task_plan.md")).unwrap_or_default(),
        progress: read_to_string(&dir.join("progress.md")).unwrap_or_default(),
        findings: read_to_string(&dir.join("findings.md")).unwrap_or_default(),
        verification: read_to_string(&dir.join("verification.md")).unwrap_or_default(),
    }
}

fn push_doc_section(content: &mut String, title: &str, body: &str) {
    content.push_str(&format!("### {}\n\n", title));
    if body.trim().is_empty() {
        content.push_str("_Not present._\n\n");
        return;
    }
    content.push_str("```markdown\n");
    content.push_str(body.trim_end());
    content.push_str("\n```\n\n");
}

fn table_cell(value: &str) -> String {
    value
        .replace('|', "\\|")
        .replace('\n', " ")
        .trim()
        .to_string()
}

fn branch_slug(branch: &str) -> String {
    let mut stem = String::new();
    for ch in branch.chars() {
        match ch {
            '/' => stem.push_str("__"),
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' => stem.push(c),
            _ => stem.push('_'),
        }
    }
    if stem.is_empty() {
        "root".to_string()
    } else {
        stem
    }
}

fn docs_dir_for_branch(root: &Path, branch: &str) -> PathBuf {
    if branch == "root" {
        return tw_dir(root);
    }
    if let Some(bound_branch) = &invocation().branch {
        if bound_branch == branch {
            let candidate = tw_dir(&invocation().workspace_root)
                .join("branches")
                .join(branch);
            if candidate.exists() {
                return candidate;
            }
        }
    }
    branch_dir(root, branch)
}

fn read_docs_dir_for_branch(root: &Path, branch: &str) -> PathBuf {
    let local = docs_dir_for_branch(root, branch);
    if local != branch_dir(root, branch) {
        return local;
    }
    if let Ok(branches) = load_branches(root) {
        if let Some(item) = branches.iter().find(|item| item.path == branch) {
            if !item.isolation.workspace_path.trim().is_empty() {
                let candidate = tw_dir(Path::new(&item.isolation.workspace_path))
                    .join("branches")
                    .join(branch);
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }
    branch_dir(root, branch)
}

fn replace_block(content: &str, key: &str, new_block: &str) -> String {
    let start_marker = format!("<!-- {}:start -->", key);
    let end_marker = format!("<!-- {}:end -->", key);
    if let (Some(start), Some(end)) = (content.find(&start_marker), content.find(&end_marker)) {
        let end_pos = end + end_marker.len();
        format!("{}{}{}", &content[..start], new_block, &content[end_pos..])
    } else {
        format!("{}\n\n{}\n", new_block, content)
    }
}

fn find_branch<'a>(branches: &'a [Branch], path: &str) -> AppResult<&'a Branch> {
    branches
        .iter()
        .find(|b| b.path == path)
        .ok_or_else(|| AppError(format!("branch `{}` does not exist", path)))
}

fn branch_is_structurally_protected(branch: &Branch) -> bool {
    matches!(branch.status.as_str(), "complete" | "aborted")
}

fn repair_parent_edges(edges: &mut Vec<Edge>, branches: &[Branch]) {
    let branch_paths: HashSet<&str> = branches.iter().map(|branch| branch.path.as_str()).collect();
    let mut used_parent_edges = HashSet::new();
    for branch in branches.iter().filter(|branch| branch.path != "root") {
        let existing_index = edges.iter().position(|edge| {
            edge.kind == "parent_of"
                && edge.to == branch.path
                && branch_paths.contains(edge.from.as_str())
                && !used_parent_edges.contains(&edge.id)
        });
        if let Some(index) = existing_index {
            let edge = &mut edges[index];
            edge.from = branch.parent.clone();
            edge.to = branch.path.clone();
            edge.kind = "parent_of".to_string();
            edge.user_label = format!("{} contains {}", branch.parent, branch.path);
            edge.interpreted_relation = "parent_of".to_string();
            used_parent_edges.insert(edge.id.clone());
        } else {
            let edge_id = next_edge_id(edges);
            edges.push(Edge {
                id: edge_id.clone(),
                from: branch.parent.clone(),
                to: branch.path.clone(),
                kind: "parent_of".to_string(),
                user_label: format!("{} contains {}", branch.parent, branch.path),
                interpreted_relation: "parent_of".to_string(),
            });
            used_parent_edges.insert(edge_id);
        }
    }
    edges.retain(|edge| edge.kind != "parent_of" || used_parent_edges.contains(&edge.id));
}

fn rewrite_branch_doc_headers(root: &Path, branch_path: &str, parent: &str) -> AppResult<()> {
    let dir = branch_dir(root, branch_path);
    rewrite_doc_fields(
        &dir.join("task_plan.md"),
        &[("Branch", branch_path), ("Parent", parent)],
    )?;
    rewrite_doc_fields(
        &dir.join("progress.md"),
        &[("Branch", branch_path), ("Parent", parent)],
    )?;
    rewrite_doc_fields(&dir.join("findings.md"), &[("Branch", branch_path)])?;
    rewrite_doc_fields(&dir.join("verification.md"), &[("Branch", branch_path)])?;
    Ok(())
}

fn rewrite_doc_fields(path: &Path, fields: &[(&str, &str)]) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let content = read_to_string(path)?;
    let mut changed = false;
    let mut lines = Vec::new();
    for line in content.lines() {
        let mut replacement = None;
        for (field, value) in fields {
            let prefix = format!("{}: ", field);
            if line.starts_with(&prefix) {
                replacement = Some(format!("{}: {}", field, value));
                break;
            }
        }
        if let Some(next_line) = replacement {
            if next_line != line {
                changed = true;
            }
            lines.push(next_line);
        } else {
            lines.push(line.to_string());
        }
    }
    if changed {
        let mut next = lines.join("\n");
        if content.ends_with('\n') {
            next.push('\n');
        }
        write_atomic(path, &next)?;
    }
    Ok(())
}

fn tw_dir(root: &Path) -> PathBuf {
    root.join(TW_DIR)
}

fn branch_dir(root: &Path, branch: &str) -> PathBuf {
    tw_dir(root).join("branches").join(branch)
}

fn lock_dir(root: &Path) -> PathBuf {
    root.join(".TreeWork.lock")
}

fn acquire_lock(root: &Path) -> AppResult<LockGuard> {
    let path = lock_dir(root);
    for _ in 0..500 {
        match fs::create_dir(&path) {
            Ok(()) => {
                let owner = std::process::id().to_string();
                if let Err(error) = fs::write(path.join("owner.pid"), &owner) {
                    let _ = fs::remove_dir_all(&path);
                    return Err(AppError(format!(
                        "cannot record TreeWork lock owner: {}",
                        error
                    )));
                }
                return Ok(LockGuard { path, owner });
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
                if stale_lock_owner(&path)? {
                    match fs::remove_dir_all(&path) {
                        Ok(()) => continue,
                        Err(remove_error) if remove_error.kind() == io::ErrorKind::NotFound => {
                            continue
                        }
                        Err(remove_error) => {
                            return Err(AppError(format!(
                                "cannot remove stale TreeWork lock: {}",
                                remove_error
                            )))
                        }
                    }
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(err) => return Err(AppError(format!("cannot acquire TreeWork lock: {}", err))),
        }
    }
    Err(AppError(
        "timed out waiting for TreeWork transaction lock".to_string(),
    ))
}

fn stale_lock_owner(path: &Path) -> AppResult<bool> {
    let owner_path = path.join("owner.pid");
    if !owner_path.exists() {
        let age = fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .unwrap_or_default();
        return Ok(age >= Duration::from_secs(30));
    }
    let owner = fs::read_to_string(&owner_path)?;
    let pid = owner.trim().parse::<u32>().map_err(|error| {
        AppError(format!(
            "invalid TreeWork lock owner {}: {}",
            owner_path.display(),
            error
        ))
    })?;
    if pid == std::process::id() {
        return Ok(false);
    }
    match Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => Ok(!status.success()),
        Err(_) => Ok(false),
    }
}

fn existing_tw(root: &Path) -> Option<PathBuf> {
    let p = tw_dir(root);
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

fn require_treework(root: &Path) -> AppResult<()> {
    if existing_tw(root).is_some() && tw_dir(root).join("state/project.json").exists() {
        recover_pending_transaction(root)?;
        rollback_pending_tree_apply(root)?;
        Ok(())
    } else {
        Err(AppError(
            "TreeWork is not initialized. Run `tw init` or `tw align start`.".to_string(),
        ))
    }
}

fn read_to_string(path: &Path) -> AppResult<String> {
    Ok(fs::read_to_string(path)?)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> AppResult<T> {
    let src = read_to_string(path)?;
    serde_json::from_str(&src)
        .map_err(|err| AppError(format!("failed to parse JSON {}: {}", path.display(), err)))
}

pub(crate) fn json_pretty_content<T: Serialize + ?Sized>(value: &T) -> AppResult<String> {
    let mut content = serde_json::to_string_pretty(value)
        .map_err(|err| AppError(format!("failed to serialize JSON: {}", err)))?;
    content.push('\n');
    Ok(content)
}

pub(crate) fn write_json_pretty<T: Serialize + ?Sized>(path: &Path, value: &T) -> AppResult<()> {
    let content = json_pretty_content(value).map_err(|err| {
        AppError(format!(
            "failed to serialize JSON {}: {}",
            path.display(),
            err.0
        ))
    })?;
    write_atomic(path, &content)
}

fn write_file_if_missing(path: &Path, content: &str) -> AppResult<()> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(path, content)?;
    }
    Ok(())
}

fn write_atomic(path: &Path, content: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, content)?;
    fs::rename(tmp, path)?;
    Ok(())
}

pub(crate) fn stable_hash_str(value: &str) -> String {
    stable_hash_bytes(value.as_bytes())
}

pub(crate) fn stable_hash_bytes(value: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in value {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv1a64:{:016x}", hash)
}

fn now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("unix:{}", secs)
}

fn default_binding_version() -> u32 {
    1
}

fn default_schema_version() -> String {
    "0.1".to_string()
}

fn default_stage() -> String {
    "alignment".to_string()
}

fn default_current_branch() -> String {
    "root".to_string()
}

fn default_branch_status() -> String {
    "pending".to_string()
}

fn default_verification_status() -> String {
    "unverified".to_string()
}

fn default_sync_status() -> String {
    "clean".to_string()
}

fn default_edge_kind() -> String {
    "related_to".to_string()
}

#[cfg(test)]
mod project_map_output_tests {
    use super::*;
    use crate::project_map_read_model::test_support::TestFixture;
    use std::collections::BTreeMap;

    fn accepted_snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        let treework = tw_dir(root);
        let mut snapshot = BTreeMap::new();
        for accepted_root in [
            treework.join("state"),
            treework.join("events.jsonl"),
            treework.join("history/checkpoints"),
        ] {
            if accepted_root.is_file() {
                snapshot.insert(
                    accepted_root
                        .strip_prefix(&treework)
                        .expect("accepted relative path")
                        .to_string_lossy()
                        .to_string(),
                    fs::read(&accepted_root).expect("read accepted file"),
                );
            } else if accepted_root.is_dir() {
                let mut pending = vec![accepted_root];
                while let Some(directory) = pending.pop() {
                    for entry in fs::read_dir(directory).expect("read accepted directory") {
                        let path = entry.expect("accepted entry").path();
                        if path.is_dir() {
                            pending.push(path);
                        } else if path.is_file() {
                            snapshot.insert(
                                path.strip_prefix(&treework)
                                    .expect("accepted relative path")
                                    .to_string_lossy()
                                    .to_string(),
                                fs::read(path).expect("read accepted file"),
                            );
                        }
                    }
                }
            }
        }
        snapshot
    }

    #[cfg(unix)]
    #[test]
    fn render_refuses_symlinked_output_root_without_external_writes() {
        use std::os::unix::fs::symlink;

        let fixture = TestFixture::accepted();
        let output = tw_dir(&fixture.root).join("out");
        if output.exists() {
            fs::remove_dir_all(&output).expect("remove fixture output");
        }
        let outside = fixture
            .root
            .parent()
            .expect("fixture parent")
            .join("outside-out");
        fs::create_dir(&outside).expect("outside output");
        fs::write(outside.join("sentinel.txt"), "keep").expect("outside sentinel");
        symlink(&outside, &output).expect("symlink output root");
        let accepted_before = accepted_snapshot(&fixture.root);

        let error = render_graph(&fixture.root).expect_err("symlinked output must fail");

        assert!(error.0.contains("refuses symlinked"));
        assert_eq!(
            fs::read_to_string(outside.join("sentinel.txt")).expect("sentinel preserved"),
            "keep"
        );
        assert!(!outside.join("graph.json").exists());
        assert!(!outside.join("project-map.html").exists());
        assert_eq!(accepted_snapshot(&fixture.root), accepted_before);
    }

    #[cfg(unix)]
    #[test]
    fn render_refuses_symlinked_critical_output_without_external_deletion() {
        use std::os::unix::fs::symlink;

        let fixture = TestFixture::accepted();
        let output = tw_dir(&fixture.root).join("out");
        fs::create_dir_all(&output).expect("fixture output");
        let outside = fixture
            .root
            .parent()
            .expect("fixture parent")
            .join("outside-vendor");
        fs::create_dir(&outside).expect("outside vendor");
        fs::write(outside.join("sentinel.txt"), "keep").expect("outside sentinel");
        symlink(&outside, output.join("vendor")).expect("symlink vendor output");
        let accepted_before = accepted_snapshot(&fixture.root);

        let error = render_graph(&fixture.root).expect_err("symlinked vendor must fail");

        assert!(error.0.contains("refuses symlinked"));
        assert_eq!(
            fs::read_to_string(outside.join("sentinel.txt")).expect("sentinel preserved"),
            "keep"
        );
        assert_eq!(accepted_snapshot(&fixture.root), accepted_before);
    }
}
