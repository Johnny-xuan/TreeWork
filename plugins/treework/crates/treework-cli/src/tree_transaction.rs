use crate::tree_diff::TreeOperation;
use crate::tree_document::{AcceptedTreeNode, TreeDocument};
use crate::{Branch, Edge, Project, TreeEditingSession};
use serde::{Deserialize, Serialize};

pub struct TreeApplyPlan {
    pub document: TreeDocument,
    pub nodes: Vec<AcceptedTreeNode>,
    pub source: String,
    pub source_hash: String,
    pub session: TreeEditingSession,
    pub operations: Vec<TreeOperation>,
    pub errors: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TreeApplyJournal {
    pub created_at: String,
    pub proposal_id: String,
    pub old_project: Project,
    pub old_branches: Vec<Branch>,
    pub old_edges: Vec<Edge>,
    pub old_events: String,
    #[serde(default)]
    pub old_tree_state: Option<String>,
    #[serde(default)]
    pub file_backups: Vec<TreeFileBackup>,
    pub operations: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct TreeFileBackup {
    pub relative_path: String,
    #[serde(default)]
    pub old_content: Option<String>,
}
