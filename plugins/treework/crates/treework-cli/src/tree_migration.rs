use crate::tree_document::{parse_tree_document, TreeDocument, TreeNode};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct LegacyTreeNode {
    pub id: String,
    pub parent: String,
    pub title: String,
    pub purpose: String,
    pub spec: Option<String>,
    pub depends_on: Vec<String>,
}

pub fn document_from_legacy(nodes: &[LegacyTreeNode]) -> Result<TreeDocument, Vec<String>> {
    let mut errors = Vec::new();
    let mut by_id = HashMap::new();
    for node in nodes {
        if by_id.insert(node.id.as_str(), node).is_some() {
            errors.push(format!(
                "legacy state contains duplicate branch `{}`",
                node.id
            ));
        }
    }
    let Some(root) = by_id.get("root").copied() else {
        errors.push("legacy state has no `root` branch".to_string());
        return Err(errors);
    };
    if !root.parent.is_empty() {
        errors.push("legacy root branch has a parent".to_string());
    }
    for node in nodes.iter().filter(|node| node.id != "root") {
        if !by_id.contains_key(node.parent.as_str()) {
            errors.push(format!(
                "legacy branch `{}` references missing parent `{}`",
                node.id, node.parent
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let mut visited = HashSet::new();
    let tree = build_node(root, nodes, &mut visited, &mut Vec::new(), &mut errors);
    for node in nodes {
        if !visited.contains(node.id.as_str()) {
            errors.push(format!(
                "legacy branch `{}` is disconnected from root or part of a parent cycle",
                node.id
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }

    let document = TreeDocument { version: 1, tree };
    let serialized = serde_saphyr::to_string(&document)
        .map_err(|error| vec![format!("cannot serialize migrated Tree: {}", error)])?;
    parse_tree_document(&serialized).map_err(|errors| {
        errors
            .iter()
            .map(|error| error.render(".TreeWork/tree.yaml"))
            .collect()
    })
}

fn build_node(
    node: &LegacyTreeNode,
    nodes: &[LegacyTreeNode],
    visited: &mut HashSet<String>,
    stack: &mut Vec<String>,
    errors: &mut Vec<String>,
) -> TreeNode {
    if stack.contains(&node.id) {
        let mut cycle = stack.clone();
        cycle.push(node.id.clone());
        errors.push(format!("legacy parent cycle: {}", cycle.join(" -> ")));
    }
    if !visited.insert(node.id.clone()) {
        return TreeNode {
            id: node.id.clone(),
            title: node.title.clone(),
            purpose: node.purpose.clone(),
            spec: node.spec.clone(),
            depends_on: node.depends_on.clone(),
            children: Vec::new(),
        };
    }
    stack.push(node.id.clone());
    let children = nodes
        .iter()
        .filter(|candidate| candidate.parent == node.id)
        .map(|child| build_node(child, nodes, visited, stack, errors))
        .collect();
    stack.pop();
    TreeNode {
        id: node.id.clone(),
        title: node.title.clone(),
        purpose: node.purpose.clone(),
        spec: node.spec.clone(),
        depends_on: node.depends_on.clone(),
        children,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree_document::accepted_nodes;

    #[test]
    fn migrates_legacy_order_and_dependencies() {
        let nodes = vec![
            LegacyTreeNode {
                id: "root".to_string(),
                parent: String::new(),
                title: "Project".to_string(),
                purpose: "Coordinate the project.".to_string(),
                spec: Some("spec.md".to_string()),
                depends_on: vec![],
            },
            LegacyTreeNode {
                id: "alpha".to_string(),
                parent: "root".to_string(),
                title: "Alpha".to_string(),
                purpose: "Build alpha.".to_string(),
                spec: None,
                depends_on: vec![],
            },
            LegacyTreeNode {
                id: "beta".to_string(),
                parent: "root".to_string(),
                title: "Beta".to_string(),
                purpose: "Build beta.".to_string(),
                spec: None,
                depends_on: vec!["alpha".to_string()],
            },
        ];
        let document = document_from_legacy(&nodes).expect("valid migration");
        let flattened = accepted_nodes(&document);
        assert_eq!(flattened[2].id, "beta");
        assert_eq!(flattened[2].sibling_order, 1);
        assert_eq!(flattened[2].depends_on, vec!["alpha"]);
    }
}
