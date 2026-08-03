use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::path::{Path, PathBuf};

pub const LEGACY_FLAT_LAYOUT: u32 = 1;
pub const HIERARCHICAL_LAYOUT: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchArtifactNode {
    pub id: String,
    pub parent: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchArtifactLayout {
    version: u32,
    relative_dirs: BTreeMap<String, PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BranchArtifactError(String);

impl BranchArtifactError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for BranchArtifactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for BranchArtifactError {}

impl BranchArtifactLayout {
    pub fn build(
        version: u32,
        nodes: impl IntoIterator<Item = BranchArtifactNode>,
    ) -> Result<Self, BranchArtifactError> {
        if !matches!(version, LEGACY_FLAT_LAYOUT | HIERARCHICAL_LAYOUT) {
            return Err(BranchArtifactError::new(format!(
                "unsupported branch artifact layout version {version}"
            )));
        }

        let mut parents = HashMap::new();
        for node in nodes {
            if parents.insert(node.id.clone(), node.parent).is_some() {
                return Err(BranchArtifactError::new(format!(
                    "duplicate branch id `{}` in artifact layout",
                    node.id
                )));
            }
        }
        if parents.get("root").is_none_or(|parent| !parent.is_empty()) {
            return Err(BranchArtifactError::new(
                "artifact layout requires branch `root` with no parent",
            ));
        }

        let mut relative_dirs = BTreeMap::new();
        relative_dirs.insert("root".to_string(), PathBuf::new());
        let mut destinations = HashMap::<PathBuf, String>::new();

        let mut ids: Vec<String> = parents.keys().cloned().collect();
        ids.sort();
        for id in ids.into_iter().filter(|id| id != "root") {
            let relative = if version == LEGACY_FLAT_LAYOUT {
                legacy_relative_dir(&id)?
            } else {
                hierarchical_relative_dir(&id, &parents)?
            };
            if let Some(existing) = destinations.insert(relative.clone(), id.clone()) {
                return Err(BranchArtifactError::new(format!(
                    "branches `{existing}` and `{id}` resolve to the same artifact directory `{}`",
                    relative.display()
                )));
            }
            relative_dirs.insert(id, relative);
        }

        Ok(Self {
            version,
            relative_dirs,
        })
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn relative_dir(&self, branch: &str) -> Result<&Path, BranchArtifactError> {
        self.relative_dirs
            .get(branch)
            .map(PathBuf::as_path)
            .ok_or_else(|| BranchArtifactError::new(format!("unknown branch `{branch}`")))
    }

    pub fn artifact_dir(
        &self,
        treework_dir: &Path,
        branch: &str,
    ) -> Result<PathBuf, BranchArtifactError> {
        Ok(treework_dir.join(self.relative_dir(branch)?))
    }

    pub fn canonical_spec_path(&self, branch: &str) -> Result<PathBuf, BranchArtifactError> {
        Ok(self.relative_dir(branch)?.join("spec.md"))
    }

    pub fn branches(&self) -> impl Iterator<Item = (&str, &Path)> {
        self.relative_dirs
            .iter()
            .map(|(id, path)| (id.as_str(), path.as_path()))
    }
}

fn legacy_relative_dir(id: &str) -> Result<PathBuf, BranchArtifactError> {
    if id.starts_with('/')
        || id
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(BranchArtifactError::new(format!(
            "legacy branch id `{id}` is not a contained relative path"
        )));
    }
    Ok(PathBuf::from("branches").join(id))
}

fn hierarchical_relative_dir(
    id: &str,
    parents: &HashMap<String, String>,
) -> Result<PathBuf, BranchArtifactError> {
    let mut chain = Vec::new();
    let mut cursor = id;
    let mut seen = HashSet::new();

    loop {
        if !seen.insert(cursor.to_string()) {
            let mut cycle: Vec<String> = seen.into_iter().collect();
            cycle.sort();
            return Err(BranchArtifactError::new(format!(
                "branch parent cycle while resolving `{id}`: {}",
                cycle.join(", ")
            )));
        }
        if cursor == "root" {
            break;
        }
        chain.push(encode_segment(cursor));
        let parent = parents.get(cursor).ok_or_else(|| {
            BranchArtifactError::new(format!(
                "branch `{cursor}` referenced while resolving `{id}` does not exist"
            ))
        })?;
        if parent.is_empty() {
            return Err(BranchArtifactError::new(format!(
                "branch `{cursor}` has no parent while resolving `{id}`"
            )));
        }
        cursor = parent;
    }

    chain.reverse();
    let mut path = PathBuf::from("branches");
    for segment in chain {
        path.push(segment);
    }
    Ok(path)
}

pub fn encode_segment(id: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(id.len());
    for byte in id.bytes() {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[(byte >> 4) as usize]));
            encoded.push(char::from(HEX[(byte & 0x0f) as usize]));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, parent: &str) -> BranchArtifactNode {
        BranchArtifactNode {
            id: id.to_string(),
            parent: parent.to_string(),
        }
    }

    #[test]
    fn hierarchical_layout_follows_parent_chain() {
        let layout = BranchArtifactLayout::build(
            HIERARCHICAL_LAYOUT,
            [
                node("root", ""),
                node("release", "root"),
                node("fix", "release"),
            ],
        )
        .unwrap();

        assert_eq!(layout.relative_dir("root").unwrap(), Path::new(""));
        assert_eq!(
            layout.relative_dir("fix").unwrap(),
            Path::new("branches/release/fix")
        );
        assert_eq!(
            layout.canonical_spec_path("fix").unwrap(),
            Path::new("branches/release/fix/spec.md")
        );
    }

    #[test]
    fn branch_id_is_one_injective_segment() {
        assert_eq!(encode_segment("api/v2"), "api%2Fv2");
        assert_eq!(encode_segment("progress.md"), "progress%2Emd");
        assert_eq!(encode_segment("api_v2-fix"), "api_v2-fix");
    }

    #[test]
    fn legacy_layout_preserves_existing_slash_behavior() {
        let layout = BranchArtifactLayout::build(
            LEGACY_FLAT_LAYOUT,
            [node("root", ""), node("api/v2", "root")],
        )
        .unwrap();
        assert_eq!(
            layout.relative_dir("api/v2").unwrap(),
            Path::new("branches/api/v2")
        );
    }

    #[test]
    fn rejects_missing_parent() {
        let error = BranchArtifactLayout::build(
            HIERARCHICAL_LAYOUT,
            [node("root", ""), node("child", "missing")],
        )
        .unwrap_err();
        assert!(error.to_string().contains("does not exist"));
    }

    #[test]
    fn rejects_parent_cycle() {
        let error = BranchArtifactLayout::build(
            HIERARCHICAL_LAYOUT,
            [node("root", ""), node("a", "b"), node("b", "a")],
        )
        .unwrap_err();
        assert!(error.to_string().contains("parent cycle"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let error = BranchArtifactLayout::build(
            HIERARCHICAL_LAYOUT,
            [node("root", ""), node("a", "root"), node("a", "root")],
        )
        .unwrap_err();
        assert!(error.to_string().contains("duplicate branch id"));
    }
}
