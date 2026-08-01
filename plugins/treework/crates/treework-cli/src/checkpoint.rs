use crate::tree_document::AcceptedTreeState;
use crate::{
    accepted_tree_state_hash, stable_hash_str, write_json_pretty, AppError, AppResult, Branch,
    Project,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointProject {
    pub stage: String,
    pub current_branch: String,
    pub tree_editing: Option<crate::TreeEditingSession>,
    pub tree_hash: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointIsolation {
    pub mode: String,
    pub workspace_path: String,
    pub git_branch: String,
    pub managed_by_treework: bool,
    pub last_status: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointBranch {
    pub id: String,
    pub parent: String,
    pub title: String,
    pub purpose: String,
    pub status: String,
    pub verification_status: String,
    pub status_reason: String,
    pub isolation: CheckpointIsolation,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeCheckpoint {
    pub schema_version: u32,
    pub event_seq: u64,
    pub captured_at: String,
    pub tree_revision: u64,
    pub checkpoint_hash: String,
    pub project: CheckpointProject,
    pub tree: Option<AcceptedTreeState>,
    pub branches: Vec<CheckpointBranch>,
}

#[derive(Serialize)]
struct CheckpointHashContent<'a> {
    schema_version: u32,
    event_seq: u64,
    captured_at: &'a str,
    tree_revision: u64,
    project: &'a CheckpointProject,
    tree: &'a Option<AcceptedTreeState>,
    branches: &'a [CheckpointBranch],
}

impl TreeCheckpoint {
    pub fn new(
        event_seq: u64,
        captured_at: String,
        project: &Project,
        tree: Option<AcceptedTreeState>,
        branches: &[Branch],
    ) -> AppResult<Self> {
        let mut checkpoint = Self {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            event_seq,
            captured_at,
            tree_revision: project.tree_revision,
            checkpoint_hash: String::new(),
            project: CheckpointProject {
                stage: project.stage.clone(),
                current_branch: project.current_branch.clone(),
                tree_editing: project.tree_editing.clone(),
                tree_hash: project.tree_hash.clone(),
            },
            tree,
            branches: branches
                .iter()
                .map(|branch| CheckpointBranch {
                    id: branch.path.clone(),
                    parent: branch.parent.clone(),
                    title: branch.title.clone(),
                    purpose: branch.purpose.clone(),
                    status: branch.status.clone(),
                    verification_status: branch.verification_status.clone(),
                    status_reason: branch.status_reason.clone(),
                    isolation: CheckpointIsolation {
                        mode: branch.isolation.mode.clone(),
                        workspace_path: branch.isolation.workspace_path.clone(),
                        git_branch: branch.isolation.git_branch.clone(),
                        managed_by_treework: branch.isolation.managed_by_treework,
                        last_status: branch.isolation.last_status.clone(),
                    },
                })
                .collect(),
        };
        checkpoint.checkpoint_hash = checkpoint.compute_hash()?;
        checkpoint.validate()?;
        Ok(checkpoint)
    }

    pub fn compute_hash(&self) -> AppResult<String> {
        let content = CheckpointHashContent {
            schema_version: self.schema_version,
            event_seq: self.event_seq,
            captured_at: &self.captured_at,
            tree_revision: self.tree_revision,
            project: &self.project,
            tree: &self.tree,
            branches: &self.branches,
        };
        Ok(stable_hash_str(&serde_json::to_string(&content)?))
    }

    pub fn validate(&self) -> AppResult<()> {
        if self.schema_version != CHECKPOINT_SCHEMA_VERSION {
            return Err(AppError(format!(
                "unsupported checkpoint schema version {}",
                self.schema_version
            )));
        }
        if self.event_seq == 0 {
            return Err(AppError(
                "checkpoint event sequence must be positive".to_string(),
            ));
        }
        let computed = self.compute_hash()?;
        if computed != self.checkpoint_hash {
            return Err(AppError(format!(
                "checkpoint hash is invalid: expected {}, found {}",
                computed, self.checkpoint_hash
            )));
        }
        if !matches!(
            self.project.stage.as_str(),
            "alignment" | "build_tree" | "work_tree"
        ) {
            return Err(AppError(format!(
                "checkpoint has unsupported project stage `{}`",
                self.project.stage
            )));
        }
        match &self.tree {
            Some(tree) => {
                if self.tree_revision == 0 {
                    return Err(AppError(
                        "revision-zero checkpoint must not contain accepted Tree state".to_string(),
                    ));
                }
                if tree.revision != self.tree_revision {
                    return Err(AppError(format!(
                        "checkpoint Tree revision {} does not match {}",
                        tree.revision, self.tree_revision
                    )));
                }
                if tree.source_hash != self.project.tree_hash {
                    return Err(AppError(
                        "checkpoint Tree source hash does not match project hash".to_string(),
                    ));
                }
                if accepted_tree_state_hash(tree)? != tree.state_hash {
                    return Err(AppError(
                        "checkpoint accepted Tree state hash is invalid".to_string(),
                    ));
                }
                let nodes: HashMap<&str, _> = tree
                    .nodes
                    .iter()
                    .map(|node| (node.id.as_str(), node))
                    .collect();
                if nodes.len() != self.branches.len() {
                    return Err(AppError(
                        "checkpoint Tree and branch lifecycle counts differ".to_string(),
                    ));
                }
                for branch in &self.branches {
                    let Some(node) = nodes.get(branch.id.as_str()) else {
                        return Err(AppError(format!(
                            "checkpoint branch `{}` is absent from accepted Tree",
                            branch.id
                        )));
                    };
                    if node.parent != branch.parent
                        || node.title != branch.title
                        || node.purpose != branch.purpose
                    {
                        return Err(AppError(format!(
                            "checkpoint branch `{}` diverges from accepted Tree metadata",
                            branch.id
                        )));
                    }
                }
            }
            None if self.tree_revision != 0 => {
                return Err(AppError(format!(
                    "checkpoint revision {} is missing accepted Tree state",
                    self.tree_revision
                )))
            }
            None => {}
        }
        let mut ids = HashSet::new();
        for branch in &self.branches {
            if branch.id.trim().is_empty() || !ids.insert(branch.id.as_str()) {
                return Err(AppError(format!(
                    "checkpoint has an empty or duplicate branch id `{}`",
                    branch.id
                )));
            }
            if !matches!(
                branch.status.as_str(),
                "pending" | "in_progress" | "paused" | "complete" | "aborted"
            ) {
                return Err(AppError(format!(
                    "checkpoint branch `{}` has unsupported status `{}`",
                    branch.id, branch.status
                )));
            }
            if !matches!(
                branch.verification_status.as_str(),
                "unverified" | "partial" | "verified" | "failed"
            ) {
                return Err(AppError(format!(
                    "checkpoint branch `{}` has unsupported verification `{}`",
                    branch.id, branch.verification_status
                )));
            }
        }
        if self.tree_revision == 0 && (self.branches.len() != 1 || !ids.contains("root")) {
            return Err(AppError(
                "revision-zero checkpoint must contain only the bootstrap root".to_string(),
            ));
        }
        if !ids.contains(self.project.current_branch.as_str()) {
            return Err(AppError(format!(
                "checkpoint current branch `{}` is missing",
                self.project.current_branch
            )));
        }
        Ok(())
    }
}

pub fn checkpoint_relative_path(tree_revision: u64, event_seq: u64) -> String {
    format!(
        "history/checkpoints/tree-r{:06}-e{:06}.json",
        tree_revision, event_seq
    )
}

pub fn write_checkpoint(root: &Path, checkpoint: &TreeCheckpoint) -> AppResult<(String, String)> {
    checkpoint.validate()?;
    let relative = checkpoint_relative_path(checkpoint.tree_revision, checkpoint.event_seq);
    validate_checkpoint_fs_path(root, &relative)?;
    let path = root.join(".TreeWork").join(&relative);
    if path.exists() {
        return Err(AppError(format!(
            "refusing to overwrite immutable checkpoint {}",
            path.display()
        )));
    }
    write_json_pretty(&path, checkpoint)?;
    Ok((relative, checkpoint.checkpoint_hash.clone()))
}

pub fn load_checkpoint(root: &Path, relative: &str) -> AppResult<TreeCheckpoint> {
    validate_checkpoint_ref(relative)?;
    validate_checkpoint_fs_path(root, relative)?;
    let path = root.join(".TreeWork").join(relative);
    let source = std::fs::read_to_string(&path)?;
    let checkpoint: TreeCheckpoint = serde_json::from_str(&source).map_err(|error| {
        AppError(format!(
            "failed to parse checkpoint {}: {}",
            path.display(),
            error
        ))
    })?;
    checkpoint.validate()?;
    Ok(checkpoint)
}

fn validate_checkpoint_fs_path(root: &Path, relative: &str) -> AppResult<()> {
    let mut cursor = root.join(".TreeWork");
    for component in Path::new(relative).components() {
        let Component::Normal(segment) = component else {
            return Err(AppError(format!(
                "checkpoint reference `{}` is not normalized",
                relative
            )));
        };
        cursor.push(segment);
        if cursor.exists() && std::fs::symlink_metadata(&cursor)?.file_type().is_symlink() {
            return Err(AppError(format!(
                "checkpoint reference `{}` crosses symlink {}",
                relative,
                cursor.display()
            )));
        }
    }
    Ok(())
}

fn validate_checkpoint_ref(relative: &str) -> AppResult<()> {
    let path = PathBuf::from(relative);
    if !path.starts_with("history/checkpoints") {
        return Err(AppError(format!(
            "checkpoint reference `{}` is outside history/checkpoints",
            relative
        )));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(AppError(format!(
            "checkpoint reference `{}` is not normalized",
            relative
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BranchIsolation, BranchScope, TreeEditingSession};

    fn project() -> Project {
        Project {
            schema_version: "0.1".to_string(),
            stage: "alignment".to_string(),
            current_branch: "root".to_string(),
            last_event_seq: 1,
            tree_revision: 0,
            tree_editing: None::<TreeEditingSession>,
            tree_hash: String::new(),
            last_sync: "unix:1".to_string(),
        }
    }

    fn branches() -> Vec<Branch> {
        vec![Branch {
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
            last_sync: "unix:1".to_string(),
        }]
    }

    #[test]
    fn revision_zero_checkpoint_is_content_validated() {
        let checkpoint =
            TreeCheckpoint::new(1, "unix:1".to_string(), &project(), None, &branches()).unwrap();
        checkpoint.validate().unwrap();
        assert_eq!(
            checkpoint_relative_path(0, 1),
            "history/checkpoints/tree-r000000-e000001.json"
        );
    }

    #[test]
    fn changed_content_invalidates_hash() {
        let mut checkpoint =
            TreeCheckpoint::new(1, "unix:1".to_string(), &project(), None, &branches()).unwrap();
        checkpoint.project.stage = "work_tree".to_string();
        assert!(checkpoint.validate().is_err());
    }
}
