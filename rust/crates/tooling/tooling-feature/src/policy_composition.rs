//! Load and atomically replace an operation's effective policy after Host review.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};
use skill_api::source::{HierarchySource, HierarchySourceError, load_selected};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceFingerprint {
    pub relative_path: String,
    pub raw_sha256: String,
    pub canonical_sha256: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicySummary {
    pub sources: Vec<SourceFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicySummaryError {
    InvalidPath(String),
    DuplicatePath(String),
    InvalidDigest(String),
}

impl PolicySummary {
    pub fn from_sources(sources: &[HierarchySource]) -> Self {
        Self {
            sources: sources
                .iter()
                .map(|source| SourceFingerprint {
                    relative_path: source.relative_path.clone(),
                    raw_sha256: source.raw_sha256.clone(),
                    canonical_sha256: source.canonical_sha256.clone(),
                })
                .collect(),
        }
    }

    pub fn validate(&self) -> Result<(), PolicySummaryError> {
        let mut seen = HashSet::new();
        for source in &self.sources {
            let path = &source.relative_path;
            if path.is_empty()
                || path.contains('\\')
                || path
                    .split('/')
                    .any(|part| part.is_empty() || part == "." || part == "..")
            {
                return Err(PolicySummaryError::InvalidPath(path.clone()));
            }
            if !seen.insert(path) {
                return Err(PolicySummaryError::DuplicatePath(path.clone()));
            }
            if [&source.raw_sha256, &source.canonical_sha256]
                .iter()
                .any(|hash| !skill_api::canonical::valid_sha256_hex(hash))
            {
                return Err(PolicySummaryError::InvalidDigest(path.clone()));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedPolicy {
    pub sources: Vec<HierarchySource>,
    pub summary: PolicySummary,
    pub change: PolicyChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyLoadError {
    InvalidPrevious(PolicySummaryError),
    Source(HierarchySourceError),
}

pub fn load_policy<F, R>(
    root: &Path,
    selected: &[&str],
    reference_applies: F,
    review_compatibility: R,
    previous: Option<&PolicySummary>,
) -> Result<LoadedPolicy, PolicyLoadError>
where
    F: Fn(&str, &str, &str) -> bool,
    R: Fn(&[HierarchySource]) -> Result<(), String>,
{
    if let Some(previous) = previous {
        previous
            .validate()
            .map_err(PolicyLoadError::InvalidPrevious)?;
    }
    let sources = load_selected(root, selected, reference_applies, review_compatibility)
        .map_err(PolicyLoadError::Source)?;
    let summary = PolicySummary::from_sources(&sources);
    let old: HashMap<_, _> = previous
        .into_iter()
        .flat_map(|previous| &previous.sources)
        .map(|source| (source.relative_path.as_str(), source.raw_sha256.as_str()))
        .collect();
    let new: HashSet<_> = summary
        .sources
        .iter()
        .map(|source| source.relative_path.as_str())
        .collect();
    let change = PolicyChange {
        added_or_invalidated: sources
            .iter()
            .filter(|source| {
                old.get(source.relative_path.as_str()) != Some(&source.raw_sha256.as_str())
            })
            .cloned()
            .collect(),
        removed: previous
            .into_iter()
            .flat_map(|previous| &previous.sources)
            .filter(|source| !new.contains(source.relative_path.as_str()))
            .map(|source| source.relative_path.clone())
            .collect(),
    };
    Ok(LoadedPolicy {
        sources,
        summary,
        change,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperationPolicy {
    sources: Vec<HierarchySource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyChange {
    /// Sources whose bodies must enter the Host's effective policy context.
    pub added_or_invalidated: Vec<HierarchySource>,
    /// Source identities that no longer apply to this operation.
    pub removed: Vec<String>,
}

impl OperationPolicy {
    pub fn sources(&self) -> &[HierarchySource] {
        &self.sources
    }

    /// Recheck bytes, applicability and the entire candidate before changing effective policy.
    /// Unchanged source bodies remain in the Host's existing context.
    pub fn update<F, R>(
        &mut self,
        root: &Path,
        selected: &[&str],
        reference_applies: F,
        review_compatibility: R,
    ) -> Result<PolicyChange, HierarchySourceError>
    where
        F: Fn(&str, &str, &str) -> bool,
        R: Fn(&[HierarchySource]) -> Result<(), String>,
    {
        let loaded = load_policy(
            root,
            selected,
            reference_applies,
            review_compatibility,
            Some(&PolicySummary::from_sources(&self.sources)),
        )
        .map_err(|failure| match failure {
            PolicyLoadError::Source(error) => error,
            PolicyLoadError::InvalidPrevious(_) => unreachable!("internal summary is valid"),
        })?;
        self.sources = loaded.sources;
        Ok(loaded.change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn node(path: &Path, body: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("instructions.md"),
            format!("---\nname: Policy\ndescription: Applicable policy.\nmetadata:\n  tags:\n    - policy\n---\n{body}"),
        )
        .unwrap();
    }

    #[test]
    fn operation_updates_only_changed_bodies_after_whole_set_review() {
        let root = std::env::temp_dir().join(format!(
            "skill-operation-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root, "Root.\n");
        node(&root.join("capabilities"), "Capability.\n");
        node(&root.join("capabilities/analysis"), "Analysis.\n");
        node(&root.join("interfaces"), "Interface.\n");
        node(&root.join("interfaces/mcp"), "MCP.\n");
        node(&root.join("families"), "Family.\n");
        node(&root.join("families/serena"), "Serena.\n");
        let mut policy = OperationPolicy::default();
        let selection = ["capabilities/analysis", "interfaces/mcp"];
        let first = policy
            .update(&root, &selection, |_, _, _| false, |_| Ok(()))
            .unwrap();
        assert_eq!(first.added_or_invalidated.len(), 5);
        assert!(first.removed.is_empty());
        let unchanged = policy
            .update(&root, &selection, |_, _, _| false, |_| Ok(()))
            .unwrap();
        assert!(unchanged.added_or_invalidated.is_empty());
        assert!(unchanged.removed.is_empty());
        let prior = policy.clone();
        let rejected = policy.update(
            &root,
            &["capabilities/analysis", "interfaces/mcp", "families/serena"],
            |_, _, _| false,
            |_| Err("mandatory conflict".into()),
        );
        assert!(matches!(
            rejected,
            Err(HierarchySourceError::MandatoryConflict(_))
        ));
        assert_eq!(policy, prior);
        let changed = policy
            .update(
                &root,
                &["capabilities/analysis", "families/serena"],
                |_, _, _| false,
                |sources| {
                    assert_eq!(sources.len(), 5);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(
            changed
                .added_or_invalidated
                .iter()
                .map(|source| source.relative_path.as_str())
                .collect::<Vec<_>>(),
            [
                "families/instructions.md",
                "families/serena/instructions.md"
            ]
        );
        assert_eq!(
            changed.removed,
            [
                "interfaces/instructions.md",
                "interfaces/mcp/instructions.md"
            ]
        );
        node(&root.join("capabilities/analysis"), "Changed analysis.\n");
        let changed_body = policy
            .update(
                &root,
                &["capabilities/analysis", "families/serena"],
                |_, _, _| false,
                |_| Ok(()),
            )
            .unwrap();
        assert_eq!(changed_body.added_or_invalidated.len(), 1);
        assert_eq!(
            changed_body.added_or_invalidated[0].relative_path,
            "capabilities/analysis/instructions.md"
        );
        assert!(changed_body.removed.is_empty());
    }

    #[test]
    fn reference_applicability_changes_atomically_and_invalid_new_source_cannot_leak() {
        let root = std::env::temp_dir().join(format!(
            "skill-operation-references-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root, "Root.\n");
        node(
            &root.join("interfaces"),
            "Interface.\n\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/session.md` | Stateful session. |\n",
        );
        let reference_dir = root.join("interfaces/references");
        fs::create_dir(&reference_dir).unwrap();
        fs::write(
            reference_dir.join("session.md"),
            "---\nname: Session\ndescription: Session rules.\nmetadata:\n  tags:\n    - session\n---\nSession.\n",
        )
        .unwrap();
        let mut policy = OperationPolicy::default();
        policy
            .update(&root, &["interfaces"], |_, _, _| false, |_| Ok(()))
            .unwrap();
        let base = policy.clone();
        let added = policy
            .update(&root, &["interfaces"], |_, _, _| true, |_| Ok(()))
            .unwrap();
        assert_eq!(added.added_or_invalidated.len(), 1);
        assert!(
            added.added_or_invalidated[0]
                .relative_path
                .ends_with("references/session.md")
        );
        assert_eq!(policy.sources().len(), base.sources().len() + 1);
        let with_reference = policy.clone();
        fs::write(reference_dir.join("session.md"), b"\xff").unwrap();
        assert!(matches!(
            policy.update(&root, &["interfaces"], |_, _, _| true, |_| Ok(())),
            Err(HierarchySourceError::InvalidUtf8(_))
        ));
        assert_eq!(policy, with_reference);
        let removed = policy
            .update(&root, &["interfaces"], |_, _, _| false, |_| Ok(()))
            .unwrap();
        assert_eq!(removed.removed, ["interfaces/references/session.md"]);
        assert!(removed.added_or_invalidated.is_empty());
    }

    #[test]
    fn host_supplied_previous_summary_controls_stateless_change_detection() {
        let root = std::env::temp_dir().join(format!(
            "tooling-policy-summary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root, "Root.\n");
        node(&root.join("interfaces"), "Interface.\n");
        let first = load_policy(&root, &["interfaces"], |_, _, _| false, |_| Ok(()), None).unwrap();
        assert_eq!(first.change.added_or_invalidated.len(), 2);
        let unchanged = load_policy(
            &root,
            &["interfaces"],
            |_, _, _| false,
            |_| Ok(()),
            Some(&first.summary),
        )
        .unwrap();
        assert!(unchanged.change.added_or_invalidated.is_empty());
        assert!(unchanged.change.removed.is_empty());
        let removed = load_policy(
            &root,
            &[],
            |_, _, _| false,
            |_| Ok(()),
            Some(&first.summary),
        )
        .unwrap();
        assert_eq!(removed.change.removed, ["interfaces/instructions.md"]);
        node(&root.join("interfaces"), "Updated.\n");
        let changed = load_policy(
            &root,
            &["interfaces"],
            |_, _, _| false,
            |_| Ok(()),
            Some(&first.summary),
        )
        .unwrap();
        assert_eq!(changed.change.added_or_invalidated.len(), 1);
        let mut invalid = first.summary;
        invalid.sources.push(invalid.sources[0].clone());
        assert!(matches!(
            load_policy(&root, &[], |_, _, _| false, |_| Ok(()), Some(&invalid)),
            Err(PolicyLoadError::InvalidPrevious(
                PolicySummaryError::DuplicatePath(_)
            ))
        ));
    }
}
