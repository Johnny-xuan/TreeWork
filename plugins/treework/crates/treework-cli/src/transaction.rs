use crate::checkpoint::load_checkpoint;
use crate::event::parse_event_log;
use crate::{
    json_pretty_content, now, stable_hash_bytes, write_json_pretty, AppError, AppResult, Project,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::fs::File;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const JOURNAL_RELATIVE_PATH: &str = ".TreeWork/state/pending-transaction.json";
const BACKUP_DIR_NAME: &str = ".TreeWork.pending-transaction-backup";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TrackedPath {
    relative_path: String,
    existed: bool,
    was_directory: bool,
    backup_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum IntendedEntry {
    Directory {
        relative_path: String,
    },
    File {
        relative_path: String,
        byte_len: u64,
        hash: String,
    },
}

impl IntendedEntry {
    fn relative_path(&self) -> &str {
        match self {
            Self::Directory { relative_path } | Self::File { relative_path, .. } => relative_path,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum IntendedPathState {
    Absent,
    File { byte_len: u64, hash: String },
    Directory { entries: Vec<IntendedEntry> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct IntendedRoot {
    relative_path: String,
    state: IntendedPathState,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct IntendedCheckpoint {
    snapshot_ref: String,
    checkpoint_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PublicationMarker {
    last_event_seq: u64,
    tree_revision: u64,
    tree_hash: String,
    project_file_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct PublicationIntent {
    marker: PublicationMarker,
    roots: Vec<IntendedRoot>,
    checkpoint: Option<IntendedCheckpoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ExternalWorktreeCleanup {
    workspace_path: String,
    git_branch: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TransactionJournal {
    schema_version: u32,
    created_at: String,
    kind: String,
    remove_treework_on_rollback: bool,
    tracked_paths: Vec<TrackedPath>,
    external_worktree_cleanup: Option<ExternalWorktreeCleanup>,
    intended: Option<PublicationIntent>,
    #[serde(default)]
    pre_marker_durable: bool,
}

pub struct PublicationTransaction {
    root: PathBuf,
    journal: TransactionJournal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryOutcome {
    None,
    FinishedForward,
    RolledBack,
}

impl PublicationTransaction {
    pub fn begin(
        root: &Path,
        kind: &str,
        paths: &[PathBuf],
        remove_treework_on_rollback: bool,
    ) -> AppResult<Self> {
        recover_pending_transaction(root)?;
        let backup_dir = backup_dir(root);
        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir)?;
        }
        fs::create_dir_all(&backup_dir)?;

        let tracked_paths = (|| {
            let mut tracked_paths = Vec::new();
            for path in transaction_tracked_paths(root, paths)? {
                let stored_path = stored_path(root, &path)?;
                let metadata = match fs::symlink_metadata(&path) {
                    Ok(metadata) => Some(metadata),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(AppError(error.to_string())),
                };
                if metadata
                    .as_ref()
                    .is_some_and(|value| value.file_type().is_symlink())
                {
                    return Err(AppError(format!(
                        "transaction backup refuses symlink {}",
                        path.display()
                    )));
                }
                let backup_name = format!("path-{:04}", tracked_paths.len());
                if let Some(metadata) = &metadata {
                    let target = backup_dir.join(&backup_name);
                    if metadata.is_dir() {
                        copy_dir_recursive_for_backup(root, &path, &target)?;
                    } else if metadata.is_file() {
                        fs::copy(&path, &target)?;
                    } else {
                        return Err(AppError(format!(
                            "transaction backup refuses non-regular path {}",
                            path.display()
                        )));
                    }
                }
                tracked_paths.push(TrackedPath {
                    relative_path: stored_path,
                    existed: metadata.is_some(),
                    was_directory: metadata.as_ref().is_some_and(|value| value.is_dir()),
                    backup_name,
                });
            }
            AppResult::Ok(tracked_paths)
        })()
        .inspect_err(|_| {
            let _ = remove_backup(root);
        })?;
        sync_tree(&backup_dir)?;
        File::open(root)?.sync_all()?;

        let journal = TransactionJournal {
            schema_version: 2,
            created_at: now(),
            kind: kind.to_string(),
            remove_treework_on_rollback,
            tracked_paths,
            external_worktree_cleanup: None,
            intended: None,
            pre_marker_durable: false,
        };
        let transaction = Self {
            root: root.to_path_buf(),
            journal,
        };
        if let Err(error) = transaction.persist_journal() {
            if remove_treework_on_rollback {
                let treework = root.join(".TreeWork");
                if treework.exists() {
                    let _ = fs::remove_dir_all(treework);
                }
            } else {
                let _ = fs::remove_file(journal_path(root));
                let _ = fs::remove_file(journal_path(root).with_extension("tmp"));
            }
            let _ = remove_backup(root);
            return Err(error);
        }
        Ok(transaction)
    }

    pub fn prepare_intent(
        &mut self,
        project: &Project,
        checkpoint: Option<(&str, &str)>,
    ) -> AppResult<()> {
        let mut roots = Vec::with_capacity(self.journal.tracked_paths.len());
        for tracked in &self.journal.tracked_paths {
            roots.push(collect_intended_root(&self.root, &tracked.relative_path)?);
        }
        roots.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        let project_content = json_pretty_content(project)?;
        self.journal.intended = Some(PublicationIntent {
            marker: PublicationMarker {
                last_event_seq: project.last_event_seq,
                tree_revision: project.tree_revision,
                tree_hash: project.tree_hash.clone(),
                project_file_hash: stable_hash_bytes(project_content.as_bytes()),
            },
            roots,
            checkpoint: checkpoint.map(|(snapshot_ref, checkpoint_hash)| IntendedCheckpoint {
                snapshot_ref: snapshot_ref.to_string(),
                checkpoint_hash: checkpoint_hash.to_string(),
            }),
        });
        self.journal.pre_marker_durable = false;
        self.persist_journal()
    }

    pub fn sync_before_marker(&mut self) -> AppResult<()> {
        let Some(intent) = &self.journal.intended else {
            return Err(AppError(
                "cannot sync transaction without publication intent".to_string(),
            ));
        };
        sync_intended_files(&self.root, intent)?;
        self.journal.pre_marker_durable = true;
        self.persist_journal()
    }

    pub fn sync_marker(&self) -> AppResult<()> {
        if !self.journal.pre_marker_durable {
            return Err(AppError(
                "cannot sync publication marker before intended files are durable".to_string(),
            ));
        }
        sync_project_marker(&self.root)
    }

    pub fn record_created_worktree(
        &mut self,
        workspace_path: &str,
        git_branch: &str,
    ) -> AppResult<()> {
        self.journal.external_worktree_cleanup = Some(ExternalWorktreeCleanup {
            workspace_path: workspace_path.to_string(),
            git_branch: git_branch.to_string(),
        });
        self.persist_journal()
    }

    pub fn finish(self) -> AppResult<()> {
        let Some(intent) = &self.journal.intended else {
            return Err(AppError(
                "cannot finish transaction without publication intent".to_string(),
            ));
        };
        if !self.journal.pre_marker_durable {
            return Err(AppError(
                "cannot finish transaction before intended files are durable".to_string(),
            ));
        }
        sync_project_marker(&self.root)?;
        if !publication_matches(&self.root, intent)? {
            return Err(AppError(
                "published files do not match transaction intent".to_string(),
            ));
        }
        remove_journal_and_backup(&self.root)
    }

    fn persist_journal(&self) -> AppResult<()> {
        let path = journal_path(&self.root);
        write_json_pretty(&path, &self.journal)?;
        File::open(&path)?.sync_all()?;
        if let Some(parent) = path.parent() {
            sync_directory_chain(parent, &self.root)?;
        }
        Ok(())
    }
}

pub fn recover_pending_transaction(root: &Path) -> AppResult<RecoveryOutcome> {
    let path = journal_path(root);
    if !path.exists() {
        let journal_tmp = path.with_extension("tmp");
        if journal_tmp.exists() {
            fs::remove_file(journal_tmp)?;
        }
        let backup = backup_dir(root);
        if backup.exists() {
            fs::remove_dir_all(backup)?;
        }
        return Ok(RecoveryOutcome::None);
    }
    let source = fs::read_to_string(&path)?;
    let journal: TransactionJournal = serde_json::from_str(&source).map_err(|error| {
        AppError(format!(
            "failed to parse transaction journal {}: {}",
            path.display(),
            error
        ))
    })?;
    if journal.schema_version != 2 {
        return Err(AppError(format!(
            "unsupported transaction journal schema version {}",
            journal.schema_version
        )));
    }
    if journal.pre_marker_durable {
        if let Some(intent) = &journal.intended {
            if publication_matches(root, intent)? {
                sync_project_marker(root)?;
                remove_journal_and_backup(root)?;
                return Ok(RecoveryOutcome::FinishedForward);
            }
        }
    }
    rollback(root, &journal)?;
    Ok(RecoveryOutcome::RolledBack)
}

fn transaction_tracked_paths(root: &Path, paths: &[PathBuf]) -> AppResult<Vec<PathBuf>> {
    let control_treework = root.join(".TreeWork");
    let mut tracked = vec![control_treework.clone()];
    let mut seen = HashSet::from([control_treework.clone()]);
    for path in paths {
        stored_path(root, path)?;
        if path.starts_with(&control_treework) {
            continue;
        }
        if seen.insert(path.clone()) {
            tracked.push(path.clone());
        }
    }
    tracked[1..].sort_by(|left, right| left.as_os_str().cmp(right.as_os_str()));
    Ok(tracked)
}

fn collect_intended_root(root: &Path, stored: &str) -> AppResult<IntendedRoot> {
    let path = resolve_stored_path(root, stored)?;
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(AppError(error.to_string())),
    };
    let state = match metadata {
        None => IntendedPathState::Absent,
        Some(metadata) if metadata.file_type().is_symlink() => {
            return Err(AppError(format!(
                "publication intent refuses symlink {}",
                path.display()
            )))
        }
        Some(metadata) if metadata.is_file() => {
            let bytes = fs::read(&path)?;
            IntendedPathState::File {
                byte_len: bytes.len() as u64,
                hash: stable_hash_bytes(&bytes),
            }
        }
        Some(metadata) if metadata.is_dir() => {
            let mut entries = Vec::new();
            collect_directory_entries(root, &path, &path, &mut entries)?;
            entries.sort_by(|left, right| left.relative_path().cmp(right.relative_path()));
            IntendedPathState::Directory { entries }
        }
        Some(_) => {
            return Err(AppError(format!(
                "publication intent refuses non-regular path {}",
                path.display()
            )))
        }
    };
    Ok(IntendedRoot {
        relative_path: stored.to_string(),
        state,
    })
}

fn collect_directory_entries(
    root: &Path,
    directory_root: &Path,
    directory: &Path,
    entries: &mut Vec<IntendedEntry>,
) -> AppResult<()> {
    let mut children = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|entry| entry.file_name());
    for child in children {
        let path = child.path();
        if excluded_from_intent(root, &path) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            return Err(AppError(format!(
                "publication intent refuses symlink {}",
                path.display()
            )));
        }
        let relative_path = normalized_relative_path(directory_root, &path)?;
        if metadata.is_dir() {
            entries.push(IntendedEntry::Directory {
                relative_path: relative_path.clone(),
            });
            collect_directory_entries(root, directory_root, &path, entries)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&path)?;
            entries.push(IntendedEntry::File {
                relative_path,
                byte_len: bytes.len() as u64,
                hash: stable_hash_bytes(&bytes),
            });
        } else {
            return Err(AppError(format!(
                "publication intent refuses non-regular path {}",
                path.display()
            )));
        }
    }
    Ok(())
}

fn publication_matches(root: &Path, intent: &PublicationIntent) -> AppResult<bool> {
    let project_path = root.join(".TreeWork/state/project.json");
    let Ok(project_bytes) = fs::read(&project_path) else {
        return Ok(false);
    };
    if stable_hash_bytes(&project_bytes) != intent.marker.project_file_hash {
        return Ok(false);
    }
    let Ok(project) = serde_json::from_slice::<Project>(&project_bytes) else {
        return Ok(false);
    };
    if project.last_event_seq != intent.marker.last_event_seq
        || project.tree_revision != intent.marker.tree_revision
        || project.tree_hash != intent.marker.tree_hash
    {
        return Ok(false);
    }
    for expected in &intent.roots {
        let Ok(actual) = collect_intended_root(root, &expected.relative_path) else {
            return Ok(false);
        };
        if actual != *expected {
            return Ok(false);
        }
    }
    let events_path = root.join(".TreeWork/events.jsonl");
    let Ok(event_bytes) = fs::read(events_path) else {
        return Ok(false);
    };
    let Ok(events) = parse_event_log(&event_bytes) else {
        return Ok(false);
    };
    if events.last().map(|event| event.seq()).unwrap_or(0) != project.last_event_seq {
        return Ok(false);
    }
    if let Some(expected) = &intent.checkpoint {
        let Ok(checkpoint) = load_checkpoint(root, &expected.snapshot_ref) else {
            return Ok(false);
        };
        if checkpoint.event_seq != project.last_event_seq
            || checkpoint.checkpoint_hash != expected.checkpoint_hash
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn rollback(root: &Path, journal: &TransactionJournal) -> AppResult<()> {
    if journal.remove_treework_on_rollback {
        let treework = root.join(".TreeWork");
        if treework.exists() {
            fs::remove_dir_all(treework)?;
        }
        File::open(root)?.sync_all()?;
        remove_backup(root)?;
        File::open(root)?.sync_all()?;
        return Ok(());
    }
    let backup = backup_dir(root);
    let control_treework = root.join(".TreeWork");
    for tracked in &journal.tracked_paths {
        let path = resolve_stored_path(root, &tracked.relative_path)?;
        let source = backup.join(&tracked.backup_name);
        if path == control_treework {
            if !tracked.existed || !tracked.was_directory {
                return Err(AppError(
                    "existing transaction journal has no restorable .TreeWork directory"
                        .to_string(),
                ));
            }
            restore_directory_exact(root, &source, &path)?;
            continue;
        }
        let temporary = path.with_extension("tmp");
        if temporary.exists() {
            remove_path(&temporary)?;
        }
        remove_path(&path)?;
        if tracked.existed {
            if tracked.was_directory {
                copy_dir_recursive(&source, &path)?;
            } else {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(source, &path)?;
            }
        } else {
            let stop = path
                .ancestors()
                .find(|ancestor| ancestor.file_name().is_some_and(|name| name == ".TreeWork"))
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.join(".TreeWork"));
            remove_empty_parents(path.parent(), &stop)?;
        }
    }
    if let Some(cleanup) = &journal.external_worktree_cleanup {
        cleanup_created_worktree(root, cleanup)?;
    }
    sync_rolled_back_paths(root, journal)?;
    remove_journal_and_backup(root)
}

fn cleanup_created_worktree(root: &Path, cleanup: &ExternalWorktreeCleanup) -> AppResult<()> {
    let workspace = PathBuf::from(&cleanup.workspace_path);
    if workspace.exists() {
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["worktree", "remove", "--force"])
            .arg(&workspace)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| {
                AppError(format!(
                    "failed to run Git cleanup for newly created worktree `{}`: {}",
                    workspace.display(),
                    error
                ))
            })?;
        if !output.status.success() {
            return Err(AppError(format!(
                "failed to clean newly created worktree `{}` after transaction rollback: {}",
                workspace.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
    }
    if !cleanup.git_branch.trim().is_empty() {
        let exists = Command::new("git")
            .arg("-C")
            .arg(root)
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{}", cleanup.git_branch),
            ])
            .status()
            .map_err(|error| {
                AppError(format!(
                    "failed to inspect newly created branch `{}` during rollback: {}",
                    cleanup.git_branch, error
                ))
            })?
            .success();
        if !exists {
            return Ok(());
        }
        let output = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(["branch", "-D", &cleanup.git_branch])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| {
                AppError(format!(
                    "failed to run Git cleanup for newly created branch `{}`: {}",
                    cleanup.git_branch, error
                ))
            })?;
        if !output.status.success() {
            return Err(AppError(format!(
                "failed to clean newly created branch `{}` after transaction rollback: {}",
                cleanup.git_branch,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
    }
    Ok(())
}

fn sync_intended_files(root: &Path, intent: &PublicationIntent) -> AppResult<()> {
    let mut directories = HashSet::new();
    for intended_root in &intent.roots {
        let path = resolve_stored_path(root, &intended_root.relative_path)?;
        match &intended_root.state {
            IntendedPathState::Absent => {}
            IntendedPathState::File { .. } => {
                sync_regular_file(&path)?;
            }
            IntendedPathState::Directory { entries } => {
                directories.insert(path.clone());
                for entry in entries {
                    let entry_path = resolve_intended_entry(&path, entry.relative_path())?;
                    match entry {
                        IntendedEntry::Directory { .. } => {
                            directories.insert(entry_path);
                        }
                        IntendedEntry::File { .. } => {
                            sync_regular_file(&entry_path)?;
                        }
                    }
                }
            }
        }
        add_parent_directories(root, &path, &mut directories)?;
    }
    let mut directories: Vec<PathBuf> = directories.into_iter().collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        sync_directory(&directory)?;
    }
    Ok(())
}

fn sync_rolled_back_paths(root: &Path, journal: &TransactionJournal) -> AppResult<()> {
    let mut directories = HashSet::new();
    for tracked in &journal.tracked_paths {
        let path = resolve_stored_path(root, &tracked.relative_path)?;
        if path.exists() {
            if path == root.join(".TreeWork") {
                sync_tree_for_transaction(root, &path)?;
            } else {
                sync_tree(&path)?;
            }
        }
        let scope = sync_scope(root, &path)?;
        let mut current = path.parent();
        while let Some(directory) = current {
            if directory.exists() {
                directories.insert(directory.to_path_buf());
            }
            if directory == scope {
                break;
            }
            current = directory.parent();
        }
    }
    let mut directories: Vec<PathBuf> = directories.into_iter().collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        sync_directory(&directory)?;
    }
    Ok(())
}

fn sync_project_marker(root: &Path) -> AppResult<()> {
    let marker = root.join(".TreeWork/state/project.json");
    File::open(&marker)?.sync_all()?;
    if let Some(parent) = marker.parent() {
        sync_directory(parent)?;
    }
    Ok(())
}

fn sync_regular_file(path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(AppError(format!(
            "transaction sync refuses non-regular file {}",
            path.display()
        )));
    }
    File::open(path)?.sync_all()?;
    Ok(())
}

fn sync_directory(path: &Path) -> AppResult<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError(format!(
            "transaction sync refuses non-directory {}",
            path.display()
        )));
    }
    File::open(path)?.sync_all()?;
    Ok(())
}

fn remove_journal_and_backup(root: &Path) -> AppResult<()> {
    let journal = journal_path(root);
    if journal.exists() {
        fs::remove_file(&journal)?;
    }
    let journal_tmp = journal.with_extension("tmp");
    if journal_tmp.exists() {
        fs::remove_file(journal_tmp)?;
    }
    if let Some(parent) = journal.parent() {
        sync_directory(parent)?;
    }
    remove_backup(root)?;
    File::open(root)?.sync_all()?;
    Ok(())
}

fn remove_backup(root: &Path) -> AppResult<()> {
    let backup = backup_dir(root);
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok(())
}

fn remove_path(path: &Path) -> AppResult<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(AppError(error.to_string())),
    };
    if metadata.is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> AppResult<()> {
    fs::create_dir_all(target)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            return Err(AppError(format!(
                "transaction backup refuses symlink {}",
                source_path.display()
            )));
        }
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(source_path, target_path)?;
        } else {
            return Err(AppError(format!(
                "transaction restore refuses non-regular path {}",
                source_path.display()
            )));
        }
    }
    Ok(())
}

fn copy_dir_recursive_for_backup(root: &Path, source: &Path, target: &Path) -> AppResult<()> {
    fs::create_dir_all(target)?;
    let mut entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let source_path = entry.path();
        if excluded_from_backup(root, &source_path) {
            continue;
        }
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            return Err(AppError(format!(
                "transaction backup refuses symlink {}",
                source_path.display()
            )));
        }
        if metadata.is_dir() {
            copy_dir_recursive_for_backup(root, &source_path, &target_path)?;
        } else if metadata.is_file() {
            fs::copy(source_path, target_path)?;
        } else {
            return Err(AppError(format!(
                "transaction backup refuses non-regular path {}",
                source_path.display()
            )));
        }
    }
    Ok(())
}

fn restore_directory_exact(root: &Path, source: &Path, target: &Path) -> AppResult<()> {
    fs::create_dir_all(target)?;
    let mut source_entries = fs::read_dir(source)?.collect::<Result<Vec<_>, _>>()?;
    source_entries.sort_by_key(|entry| entry.file_name());
    let source_names: HashSet<_> = source_entries
        .iter()
        .map(|entry| entry.file_name())
        .collect();

    let mut target_entries = fs::read_dir(target)?.collect::<Result<Vec<_>, _>>()?;
    target_entries.sort_by_key(|entry| entry.file_name());
    for entry in target_entries {
        let path = entry.path();
        if excluded_from_backup(root, &path) {
            continue;
        }
        if !source_names.contains(&entry.file_name()) {
            remove_path(&path)?;
        }
    }

    for entry in source_entries {
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path)?;
        if metadata.file_type().is_symlink() {
            return Err(AppError(format!(
                "transaction restore refuses symlink {}",
                source_path.display()
            )));
        }
        if metadata.is_dir() {
            if fs::symlink_metadata(&target_path)
                .is_ok_and(|target_metadata| !target_metadata.is_dir())
            {
                remove_path(&target_path)?;
            }
            fs::create_dir_all(&target_path)?;
            restore_directory_exact(root, &source_path, &target_path)?;
        } else if metadata.is_file() {
            remove_path(&target_path)?;
            fs::copy(&source_path, &target_path)?;
        } else {
            return Err(AppError(format!(
                "transaction restore refuses non-regular path {}",
                source_path.display()
            )));
        }
    }
    Ok(())
}

fn sync_tree(path: &Path) -> AppResult<()> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        File::open(path)?.sync_all()?;
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry.file_type()?.is_dir() {
            sync_tree(&entry_path)?;
        } else {
            File::open(entry_path)?.sync_all()?;
        }
    }
    File::open(path)?.sync_all()?;
    Ok(())
}

fn sync_tree_for_transaction(root: &Path, path: &Path) -> AppResult<()> {
    if !path.exists() || excluded_from_backup(root, path) {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(AppError(format!(
            "transaction sync refuses symlink {}",
            path.display()
        )));
    }
    if !metadata.is_dir() {
        File::open(path)?.sync_all()?;
        return Ok(());
    }
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        sync_tree_for_transaction(root, &entry.path())?;
    }
    File::open(path)?.sync_all()?;
    Ok(())
}

fn remove_empty_parents(mut current: Option<&Path>, stop: &Path) -> AppResult<()> {
    while let Some(path) = current {
        if path == stop || !path.starts_with(stop) || !path.exists() {
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

fn excluded_from_backup(root: &Path, path: &Path) -> bool {
    let out = root.join(".TreeWork/out");
    path == journal_path(root)
        || path == journal_path(root).with_extension("tmp")
        || path == out
        || path.starts_with(&out)
}

fn excluded_from_intent(root: &Path, path: &Path) -> bool {
    let marker = root.join(".TreeWork/state/project.json");
    excluded_from_backup(root, path) || path == marker
}

fn normalized_relative_path(root: &Path, path: &Path) -> AppResult<String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        AppError(format!(
            "transaction path {} is outside {}",
            path.display(),
            root.display()
        ))
    })?;
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError(format!(
            "transaction path {} is not normalized",
            path.display()
        )));
    }
    relative
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| AppError(format!("transaction path {} is not UTF-8", path.display())))
}

fn resolve_intended_entry(root: &Path, relative: &str) -> AppResult<PathBuf> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError(format!(
            "intended transaction entry {} is not normalized",
            relative.display()
        )));
    }
    Ok(root.join(relative))
}

fn add_parent_directories(
    root: &Path,
    path: &Path,
    directories: &mut HashSet<PathBuf>,
) -> AppResult<()> {
    let scope = sync_scope(root, path)?;
    let mut current = path.parent();
    while let Some(directory) = current {
        if directory.exists() {
            directories.insert(directory.to_path_buf());
        }
        if directory == scope {
            return Ok(());
        }
        current = directory.parent();
    }
    Err(AppError(format!(
        "transaction path {} is outside sync scope {}",
        path.display(),
        scope.display()
    )))
}

fn stored_path(root: &Path, path: &Path) -> AppResult<String> {
    let value = match path.strip_prefix(root) {
        Ok(relative) => relative.to_path_buf(),
        Err(_) => {
            crate::validate_transaction_external_path(root, path)?;
            path.to_path_buf()
        }
    };
    if !value.is_absolute()
        && value
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError(format!(
            "transaction path {} is not normalized",
            path.display()
        )));
    }
    Ok(value.to_string_lossy().to_string())
}

fn resolve_stored_path(root: &Path, stored: &str) -> AppResult<PathBuf> {
    let path = PathBuf::from(stored);
    if path.is_absolute() {
        crate::validate_transaction_external_path(root, &path)?;
        Ok(path)
    } else {
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(AppError(format!(
                "stored transaction path {} is not normalized",
                path.display()
            )));
        }
        Ok(root.join(path))
    }
}

fn sync_scope(root: &Path, path: &Path) -> AppResult<PathBuf> {
    if path.starts_with(root) {
        return Ok(root.to_path_buf());
    }
    let treework = path
        .ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == ".TreeWork"))
        .ok_or_else(|| {
            AppError(format!(
                "external transaction path {} is outside a TreeWork workspace",
                path.display()
            ))
        })?;
    treework.parent().map(Path::to_path_buf).ok_or_else(|| {
        AppError(format!(
            "external transaction path {} has no workspace root",
            path.display()
        ))
    })
}

fn sync_directory_chain(start: &Path, stop: &Path) -> AppResult<()> {
    let mut current = Some(start);
    while let Some(directory) = current {
        sync_directory(directory)?;
        if directory == stop {
            return Ok(());
        }
        current = directory.parent();
    }
    Err(AppError(format!(
        "directory {} is outside transaction root {}",
        start.display(),
        stop.display()
    )))
}

fn journal_path(root: &Path) -> PathBuf {
    root.join(JOURNAL_RELATIVE_PATH)
}

fn backup_dir(root: &Path) -> PathBuf {
    root.join(BACKUP_DIR_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "treework-transaction-{}-{}-{}",
            name,
            std::process::id(),
            unique
        ))
    }

    #[test]
    fn intended_directory_is_complete_and_tracks_absence() {
        let root = test_root("manifest");
        let treework = root.join(".TreeWork");
        fs::create_dir_all(treework.join("state")).unwrap();
        fs::create_dir_all(treework.join("out")).unwrap();
        fs::create_dir_all(treework.join("empty")).unwrap();
        fs::write(treework.join("document.md"), "document\n").unwrap();
        fs::write(treework.join("state/project.json"), "{}\n").unwrap();
        fs::write(journal_path(&root), "{}\n").unwrap();
        fs::write(treework.join("out/disposable.json"), "{}\n").unwrap();

        let first = collect_intended_root(&root, ".TreeWork").unwrap();
        let IntendedPathState::Directory { entries } = &first.state else {
            panic!("expected directory intent");
        };
        let paths: HashSet<&str> = entries.iter().map(IntendedEntry::relative_path).collect();
        assert!(paths.contains("document.md"));
        assert!(paths.contains("empty"));
        assert!(paths.contains("state"));
        assert!(!paths.contains("state/project.json"));
        assert!(!paths.contains("state/pending-transaction.json"));
        assert!(!paths.contains("out"));

        let absent = collect_intended_root(&root, ".TreeWork/removed.md").unwrap();
        assert!(matches!(absent.state, IntendedPathState::Absent));

        fs::write(treework.join("later.md"), "later\n").unwrap();
        let second = collect_intended_root(&root, ".TreeWork").unwrap();
        assert_ne!(first, second);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn intended_directory_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let root = test_root("symlink");
        let treework = root.join(".TreeWork");
        fs::create_dir_all(&treework).unwrap();
        fs::write(treework.join("target.md"), "target\n").unwrap();
        symlink("target.md", treework.join("linked.md")).unwrap();

        let error = collect_intended_root(&root, ".TreeWork").unwrap_err();
        assert!(error.0.contains("refuses symlink"));
        fs::remove_dir_all(root).unwrap();
    }
}
