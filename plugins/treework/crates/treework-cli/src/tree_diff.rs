use crate::tree_document::{AcceptedTreeNode, AcceptedTreeState};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TreeOperation {
    CreateBranch {
        branch: String,
        parent: String,
        sibling_order: usize,
    },
    MoveBranch {
        branch: String,
        from: String,
        to: String,
    },
    UpdateMetadata {
        branch: String,
        fields: Vec<String>,
    },
    ReorderBranch {
        branch: String,
        from: usize,
        to: usize,
    },
    AddDependency {
        branch: String,
        depends_on: String,
    },
    RemoveDependency {
        branch: String,
        depends_on: String,
    },
}

impl TreeOperation {
    pub fn subject(&self) -> &str {
        match self {
            Self::CreateBranch { branch, .. }
            | Self::MoveBranch { branch, .. }
            | Self::UpdateMetadata { branch, .. }
            | Self::ReorderBranch { branch, .. }
            | Self::AddDependency { branch, .. }
            | Self::RemoveDependency { branch, .. } => branch,
        }
    }
}

pub fn diff_tree(
    accepted: Option<&AcceptedTreeState>,
    candidate: &[AcceptedTreeNode],
) -> Vec<TreeOperation> {
    let current: HashMap<&str, &AcceptedTreeNode> = accepted
        .map(|tree| {
            tree.nodes
                .iter()
                .map(|node| (node.id.as_str(), node))
                .collect()
        })
        .unwrap_or_default();
    let mut operations = Vec::new();

    for node in candidate {
        let Some(previous) = current.get(node.id.as_str()).copied() else {
            operations.push(TreeOperation::CreateBranch {
                branch: node.id.clone(),
                parent: node.parent.clone(),
                sibling_order: node.sibling_order,
            });
            for dependency in &node.depends_on {
                operations.push(TreeOperation::AddDependency {
                    branch: node.id.clone(),
                    depends_on: dependency.clone(),
                });
            }
            continue;
        };

        if previous.parent != node.parent {
            operations.push(TreeOperation::MoveBranch {
                branch: node.id.clone(),
                from: previous.parent.clone(),
                to: node.parent.clone(),
            });
        }
        let mut fields = Vec::new();
        if previous.title != node.title {
            fields.push("title".to_string());
        }
        if previous.purpose != node.purpose {
            fields.push("purpose".to_string());
        }
        if previous.spec != node.spec {
            fields.push("spec".to_string());
        }
        if !fields.is_empty() {
            operations.push(TreeOperation::UpdateMetadata {
                branch: node.id.clone(),
                fields,
            });
        }
        if previous.parent == node.parent && previous.sibling_order != node.sibling_order {
            operations.push(TreeOperation::ReorderBranch {
                branch: node.id.clone(),
                from: previous.sibling_order,
                to: node.sibling_order,
            });
        }

        let previous_dependencies: HashSet<&str> =
            previous.depends_on.iter().map(String::as_str).collect();
        let candidate_dependencies: HashSet<&str> =
            node.depends_on.iter().map(String::as_str).collect();
        for dependency in &previous.depends_on {
            if !candidate_dependencies.contains(dependency.as_str()) {
                operations.push(TreeOperation::RemoveDependency {
                    branch: node.id.clone(),
                    depends_on: dependency.clone(),
                });
            }
        }
        for dependency in &node.depends_on {
            if !previous_dependencies.contains(dependency.as_str()) {
                operations.push(TreeOperation::AddDependency {
                    branch: node.id.clone(),
                    depends_on: dependency.clone(),
                });
            }
        }
    }
    operations
}

pub fn omitted_branch_ids(
    accepted: Option<&AcceptedTreeState>,
    candidate: &[AcceptedTreeNode],
) -> Vec<String> {
    let candidate_ids: HashSet<&str> = candidate.iter().map(|node| node.id.as_str()).collect();
    let mut omitted: Vec<String> = accepted
        .into_iter()
        .flat_map(|tree| tree.nodes.iter())
        .filter(|node| !candidate_ids.contains(node.id.as_str()))
        .map(|node| node.id.clone())
        .collect();
    omitted.sort();
    omitted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, parent: &str, order: usize, depends_on: &[&str]) -> AcceptedTreeNode {
        AcceptedTreeNode {
            id: id.to_string(),
            parent: parent.to_string(),
            title: id.to_string(),
            purpose: format!("{} purpose", id),
            spec: None,
            sibling_order: order,
            depends_on: depends_on.iter().map(|item| item.to_string()).collect(),
        }
    }

    #[test]
    fn derives_metadata_move_order_and_dependency_changes() {
        let accepted = AcceptedTreeState {
            schema_version: 1,
            revision: 1,
            source_hash: "source".to_string(),
            state_hash: "state".to_string(),
            accepted_at: "now".to_string(),
            root: "root".to_string(),
            nodes: vec![
                node("root", "", 0, &[]),
                node("alpha", "root", 0, &[]),
                node("beta", "root", 1, &["alpha"]),
            ],
        };
        let mut root = node("root", "", 0, &[]);
        root.title = "Project".to_string();
        root.spec = Some("spec.md".to_string());
        let candidate = vec![
            root,
            node("beta", "root", 0, &[]),
            node("alpha", "beta", 0, &[]),
            node("gamma", "root", 1, &["beta"]),
        ];
        let operations = diff_tree(Some(&accepted), &candidate);
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, TreeOperation::UpdateMetadata { branch, .. } if branch == "root")));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, TreeOperation::MoveBranch { branch, .. } if branch == "alpha")));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, TreeOperation::ReorderBranch { branch, .. } if branch == "beta")));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, TreeOperation::RemoveDependency { branch, .. } if branch == "beta")));
        assert!(operations
            .iter()
            .any(|operation| matches!(operation, TreeOperation::CreateBranch { branch, .. } if branch == "gamma")));
    }

    #[test]
    fn reports_omitted_accepted_branches() {
        let accepted = AcceptedTreeState {
            schema_version: 1,
            revision: 1,
            source_hash: String::new(),
            state_hash: String::new(),
            accepted_at: String::new(),
            root: "root".to_string(),
            nodes: vec![node("root", "", 0, &[]), node("old", "root", 0, &[])],
        };
        assert_eq!(
            omitted_branch_ids(Some(&accepted), &[node("root", "", 0, &[])]),
            vec!["old"]
        );
    }
}
