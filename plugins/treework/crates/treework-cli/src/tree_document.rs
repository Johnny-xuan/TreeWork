use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

const MAX_TREE_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TREE_BRANCH_DEPTH: usize = 48;
const MAX_YAML_STRUCTURAL_DEPTH: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeDocument {
    pub version: u32,
    pub tree: TreeNode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreeNode {
    pub id: String,
    pub title: String,
    pub purpose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedTreeState {
    pub schema_version: u32,
    pub revision: u64,
    pub source_hash: String,
    pub state_hash: String,
    pub accepted_at: String,
    pub root: String,
    pub nodes: Vec<AcceptedTreeNode>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedTreeNode {
    pub id: String,
    pub parent: String,
    pub title: String,
    pub purpose: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<String>,
    pub sibling_order: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeDocumentError {
    pub path: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub message: String,
    pub snippet: Option<String>,
}

impl TreeDocumentError {
    fn semantic(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            line: None,
            column: None,
            message: message.into(),
            snippet: None,
        }
    }

    pub fn render(&self, source_name: &str) -> String {
        let location = match (self.line, self.column) {
            (Some(line), Some(column)) => format!("{}:{}:{}", source_name, line, column),
            (Some(line), None) => format!("{}:{}", source_name, line),
            _ => source_name.to_string(),
        };
        let mut rendered = format!("{} [{}]: {}", location, self.path, self.message);
        if let Some(snippet) = &self.snippet {
            rendered.push_str(&format!("\n  {}", snippet.trim_end()));
        }
        rendered
    }
}

pub fn parse_tree_document(source: &str) -> Result<TreeDocument, Vec<TreeDocumentError>> {
    if source.len() > MAX_TREE_SOURCE_BYTES {
        return Err(vec![TreeDocumentError::semantic(
            "$",
            format!(
                "Tree document exceeds the {} MiB source limit",
                MAX_TREE_SOURCE_BYTES / 1024 / 1024
            ),
        )]);
    }

    let options = serde_saphyr::options! {
        budget: serde_saphyr::budget! {
            max_depth: MAX_YAML_STRUCTURAL_DEPTH,
            max_events: 250_000,
            max_nodes: 50_000,
            max_aliases: 0,
            max_anchors: 0,
            max_documents: 1,
            max_total_scalar_bytes: MAX_TREE_SOURCE_BYTES,
            max_total_comment_bytes: 2 * 1024 * 1024,
            max_merge_keys: 0,
        },
        merge_keys: serde_saphyr::MergeKeyPolicy::Error,
    };
    let document: TreeDocument =
        serde_saphyr::from_str_with_options(source, options).map_err(|error| {
            let location = error.location().map(|location| {
                (
                    usize::try_from(location.line()).unwrap_or(usize::MAX),
                    usize::try_from(location.column()).unwrap_or(usize::MAX),
                )
            });
            let snippet = location
                .and_then(|(line, _)| source.lines().nth(line.saturating_sub(1)))
                .map(str::to_string);
            vec![TreeDocumentError {
                path: "$".to_string(),
                line: location.map(|value| value.0),
                column: location.map(|value| value.1),
                message: error.to_string(),
                snippet,
            }]
        })?;

    let errors = validate_tree_document(&document);
    if errors.is_empty() {
        Ok(document)
    } else {
        Err(errors)
    }
}

pub fn serialize_tree_document(document: &TreeDocument) -> Result<String, String> {
    serde_saphyr::to_string(document).map_err(|error| error.to_string())
}

pub fn accepted_nodes(document: &TreeDocument) -> Vec<AcceptedTreeNode> {
    let mut nodes = Vec::new();
    flatten_node(&document.tree, "", 0, &mut nodes);
    nodes
}

fn flatten_node(
    node: &TreeNode,
    parent: &str,
    sibling_order: usize,
    output: &mut Vec<AcceptedTreeNode>,
) {
    output.push(AcceptedTreeNode {
        id: node.id.clone(),
        parent: parent.to_string(),
        title: node.title.clone(),
        purpose: node.purpose.clone(),
        spec: node.spec.clone(),
        sibling_order,
        depends_on: node.depends_on.clone(),
    });
    for (order, child) in node.children.iter().enumerate() {
        flatten_node(child, &node.id, order, output);
    }
}

fn validate_tree_document(document: &TreeDocument) -> Vec<TreeDocumentError> {
    let mut errors = Vec::new();
    if document.version != 1 {
        errors.push(TreeDocumentError::semantic(
            "version",
            format!(
                "unsupported Tree document version {}; expected 1",
                document.version
            ),
        ));
    }
    if document.tree.id != "root" {
        errors.push(TreeDocumentError::semantic(
            "tree.id",
            "the single top-level branch id must be `root`",
        ));
    }
    if !document.tree.depends_on.is_empty() {
        errors.push(TreeDocumentError::semantic(
            "tree.depends_on",
            "root cannot depend on another branch",
        ));
    }

    let mut ids = HashMap::new();
    validate_node(&document.tree, "tree", 0, &mut ids, &mut errors);
    validate_dependency_endpoints(&document.tree, "tree", &ids, &mut errors);
    validate_dependency_cycles(&document.tree, &ids, &mut errors);
    errors
}

fn validate_node(
    node: &TreeNode,
    path: &str,
    depth: usize,
    ids: &mut HashMap<String, String>,
    errors: &mut Vec<TreeDocumentError>,
) {
    if depth > MAX_TREE_BRANCH_DEPTH {
        errors.push(TreeDocumentError::semantic(
            path,
            format!(
                "branch nesting exceeds the supported depth of {}",
                MAX_TREE_BRANCH_DEPTH
            ),
        ));
        return;
    }

    if let Some(message) = invalid_branch_id(&node.id) {
        errors.push(TreeDocumentError::semantic(format!("{}.id", path), message));
    }
    if let Some(first_path) = ids.insert(node.id.clone(), path.to_string()) {
        errors.push(TreeDocumentError::semantic(
            format!("{}.id", path),
            format!(
                "duplicate branch id `{}`; first declared at {}.id",
                node.id, first_path
            ),
        ));
    }
    validate_text(
        &node.title,
        160,
        false,
        &format!("{}.title", path),
        "title",
        errors,
    );
    validate_text(
        &node.purpose,
        500,
        true,
        &format!("{}.purpose", path),
        "purpose",
        errors,
    );
    if let Some(spec) = &node.spec {
        if let Some(message) = invalid_spec_path(spec) {
            errors.push(TreeDocumentError::semantic(
                format!("{}.spec", path),
                message,
            ));
        }
    }

    let mut dependencies = HashSet::new();
    for (index, dependency) in node.depends_on.iter().enumerate() {
        if !dependencies.insert(dependency) {
            errors.push(TreeDocumentError::semantic(
                format!("{}.depends_on[{}]", path, index),
                format!("duplicate dependency `{}`", dependency),
            ));
        }
        if dependency == &node.id {
            errors.push(TreeDocumentError::semantic(
                format!("{}.depends_on[{}]", path, index),
                "a branch cannot depend on itself",
            ));
        }
    }

    for (index, child) in node.children.iter().enumerate() {
        validate_node(
            child,
            &format!("{}.children[{}]", path, index),
            depth + 1,
            ids,
            errors,
        );
    }
}

fn validate_text(
    value: &str,
    max_chars: usize,
    one_line: bool,
    path: &str,
    label: &str,
    errors: &mut Vec<TreeDocumentError>,
) {
    if value.trim().is_empty() {
        errors.push(TreeDocumentError::semantic(
            path,
            format!("{} must not be empty", label),
        ));
    }
    if value.chars().count() > max_chars {
        errors.push(TreeDocumentError::semantic(
            path,
            format!("{} exceeds {} characters", label, max_chars),
        ));
    }
    if one_line && value.lines().count() > 1 {
        errors.push(TreeDocumentError::semantic(
            path,
            format!("{} must be one concise line", label),
        ));
    }
}

fn validate_dependency_endpoints(
    node: &TreeNode,
    path: &str,
    ids: &HashMap<String, String>,
    errors: &mut Vec<TreeDocumentError>,
) {
    for (index, dependency) in node.depends_on.iter().enumerate() {
        if !ids.contains_key(dependency) {
            errors.push(TreeDocumentError::semantic(
                format!("{}.depends_on[{}]", path, index),
                format!("dependency `{}` does not exist in this Tree", dependency),
            ));
        }
    }
    for (index, child) in node.children.iter().enumerate() {
        validate_dependency_endpoints(child, &format!("{}.children[{}]", path, index), ids, errors);
    }
}

fn validate_dependency_cycles(
    root: &TreeNode,
    ids: &HashMap<String, String>,
    errors: &mut Vec<TreeDocumentError>,
) {
    let nodes = accepted_nodes(&TreeDocument {
        version: 1,
        tree: root.clone(),
    });
    let dependencies: HashMap<&str, Vec<&str>> = nodes
        .iter()
        .map(|node| {
            (
                node.id.as_str(),
                node.depends_on.iter().map(String::as_str).collect(),
            )
        })
        .collect();
    let mut marks: HashMap<&str, u8> = HashMap::new();
    let mut stack = Vec::new();

    for id in dependencies.keys().copied() {
        if marks.get(id).copied().unwrap_or_default() == 0 {
            visit_dependency(id, &dependencies, &mut marks, &mut stack, ids, errors);
        }
    }
}

fn visit_dependency<'a>(
    id: &'a str,
    dependencies: &HashMap<&'a str, Vec<&'a str>>,
    marks: &mut HashMap<&'a str, u8>,
    stack: &mut Vec<&'a str>,
    paths: &HashMap<String, String>,
    errors: &mut Vec<TreeDocumentError>,
) {
    marks.insert(id, 1);
    stack.push(id);
    for dependency in dependencies.get(id).into_iter().flatten().copied() {
        match marks.get(dependency).copied().unwrap_or_default() {
            0 => visit_dependency(dependency, dependencies, marks, stack, paths, errors),
            1 => {
                let start = stack
                    .iter()
                    .position(|item| *item == dependency)
                    .unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(dependency);
                errors.push(TreeDocumentError::semantic(
                    format!(
                        "{}.depends_on",
                        paths.get(id).map(String::as_str).unwrap_or("tree")
                    ),
                    format!("dependency cycle: {}", cycle.join(" -> ")),
                ));
            }
            _ => {}
        }
    }
    stack.pop();
    marks.insert(id, 2);
}

fn invalid_branch_id(id: &str) -> Option<String> {
    if id.is_empty() {
        return Some("branch id must not be empty".to_string());
    }
    if id.chars().count() > 160 {
        return Some("branch id exceeds 160 characters".to_string());
    }
    let mut chars = id.chars();
    if !chars
        .next()
        .is_some_and(|value| value.is_ascii_lowercase() || value.is_ascii_digit())
    {
        return Some("branch id must start with a lowercase ASCII letter or digit".to_string());
    }
    if !id.chars().all(|value| {
        value.is_ascii_lowercase()
            || value.is_ascii_digit()
            || matches!(value, '.' | '_' | '/' | '-')
    }) {
        return Some(
            "branch id may contain only lowercase ASCII letters, digits, `.`, `_`, `/`, and `-`"
                .to_string(),
        );
    }
    if id.ends_with('/')
        || id
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return Some("branch id must contain safe, non-empty path segments".to_string());
    }
    None
}

fn invalid_spec_path(spec: &str) -> Option<String> {
    if spec.trim().is_empty() {
        return Some("Spec path must not be empty".to_string());
    }
    if spec.chars().count() > 500 {
        return Some("Spec path exceeds 500 characters".to_string());
    }
    if spec.contains('\\') {
        return Some("Spec path must use `/` separators".to_string());
    }
    let path = Path::new(spec);
    if path.is_absolute() || !spec.ends_with(".md") {
        return Some("Spec path must be a relative `.md` path inside `.TreeWork/`".to_string());
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Some("Spec path must not contain `.` or `..` segments".to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_flattens_valid_tree() {
        let source = r#"version: 1
tree:
  id: root
  title: Example
  purpose: Coordinate the project.
  spec: spec.md
  children:
    - id: foundation
      title: Foundation
      purpose: Establish shared state.
    - id: interface
      title: Interface
      purpose: Build the user surface.
      depends_on:
        - foundation
"#;
        let document = parse_tree_document(source).expect("valid tree");
        let nodes = accepted_nodes(&document);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes[2].parent, "root");
        assert_eq!(nodes[2].sibling_order, 1);
        assert_eq!(nodes[2].depends_on, vec!["foundation"]);
    }

    #[test]
    fn rejects_unknown_fields_and_reports_location() {
        let source =
            "version: 1\ntree:\n  id: root\n  title: Root\n  purpose: Root.\n  status: pending\n";
        let errors = parse_tree_document(source).expect_err("unknown field");
        assert!(errors[0].line.is_some());
        assert!(errors[0].message.contains("unknown field"));
    }

    #[test]
    fn rejects_duplicate_ids_missing_dependencies_and_cycles() {
        let source = r#"version: 1
tree:
  id: root
  title: Root
  purpose: Root.
  children:
    - id: alpha
      title: Alpha
      purpose: Alpha.
      depends_on: [beta]
    - id: alpha
      title: Duplicate
      purpose: Duplicate.
      depends_on: [missing]
"#;
        let errors = parse_tree_document(source).expect_err("invalid tree");
        let rendered = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("duplicate branch id"));
        assert!(rendered.contains("dependency `beta` does not exist"));
        assert!(rendered.contains("dependency `missing` does not exist"));
    }

    #[test]
    fn rejects_dependency_cycle_and_unsafe_spec() {
        let source = r#"version: 1
tree:
  id: root
  title: Root
  purpose: Root.
  children:
    - id: alpha
      title: Alpha
      purpose: Alpha.
      spec: ../escape.md
      depends_on: [beta]
    - id: beta
      title: Beta
      purpose: Beta.
      depends_on: [alpha]
"#;
        let errors = parse_tree_document(source).expect_err("invalid tree");
        let rendered = errors
            .iter()
            .map(|error| error.message.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("Spec path"));
        assert!(rendered.contains("dependency cycle"));
    }

    #[test]
    fn parses_deeply_nested_tree_without_losing_order() {
        let source = nested_tree_source(32);
        let parsed = parse_tree_document(&source).expect("parse deep tree");
        let nodes = accepted_nodes(&parsed);
        assert_eq!(nodes.len(), 33);
        assert_eq!(nodes.last().map(|node| node.id.as_str()), Some("level-31"));
    }

    #[test]
    fn rejects_tree_beyond_supported_branch_depth() {
        let source = nested_tree_source(MAX_TREE_BRANCH_DEPTH + 2);
        let errors = parse_tree_document(&source).expect_err("tree is too deep");
        assert!(errors
            .iter()
            .any(|error| error.message.contains("branch nesting exceeds")));
    }

    #[test]
    fn parser_budget_rejects_pathological_yaml_depth_without_overflowing() {
        let source = nested_tree_source(80);
        let errors = parse_tree_document(&source).expect_err("YAML is pathologically deep");
        let rendered = errors
            .iter()
            .map(|error| error.message.to_ascii_lowercase())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("budget") || rendered.contains("depth"));
    }

    fn nested_tree_source(depth: usize) -> String {
        let mut source =
            "version: 1\ntree:\n  id: root\n  title: Root\n  purpose: Coordinate the tree.\n"
                .to_string();
        if depth == 0 {
            return source;
        }
        source.push_str("  children:\n");
        for level in 0..depth {
            let dash_indent = " ".repeat(4 + level * 4);
            let field_indent = " ".repeat(6 + level * 4);
            source.push_str(&format!(
                "{dash_indent}- id: level-{level}\n\
                 {field_indent}title: Level {level}\n\
                 {field_indent}purpose: Own level {level}.\n"
            ));
            if level + 1 < depth {
                source.push_str(&format!("{field_indent}children:\n"));
            }
        }
        source
    }
}
