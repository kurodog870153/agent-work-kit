use std::path::{Path, PathBuf};

use skill_api::discovery::DiscoveredChild;
use skill_api::source::HierarchySource;

pub use skill_api::discovery::DiscoveryError;
pub use skill_api::hierarchy::PathSelectionError;
pub use tooling_feature::policy_composition::{LoadedPolicy, PolicyLoadError, PolicySummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyRootError {
    SkillRootUnavailable,
    SkillIdentityInvalid,
    SkillEntrypointEscaped,
    HierarchyUnavailable,
    HierarchyEscaped,
}

pub fn hierarchy_root(raw: &str) -> Result<PathBuf, PolicyRootError> {
    let root = Path::new(raw)
        .canonicalize()
        .map_err(|_| PolicyRootError::SkillRootUnavailable)?;
    if !root.is_dir() {
        return Err(PolicyRootError::SkillRootUnavailable);
    }
    let entrypoint = root.join("SKILL.md");
    let resolved_entrypoint = entrypoint
        .canonicalize()
        .map_err(|_| PolicyRootError::SkillIdentityInvalid)?;
    if !resolved_entrypoint.is_file() || resolved_entrypoint.parent() != Some(root.as_path()) {
        return Err(PolicyRootError::SkillEntrypointEscaped);
    }
    let frontmatter = skill_api::catalog::read_frontmatter(&entrypoint)
        .map_err(|_| PolicyRootError::SkillIdentityInvalid)?;
    let header = skill_api::catalog::metadata_header(&frontmatter)
        .map_err(|_| PolicyRootError::SkillIdentityInvalid)?;
    if header.name != "tooling" {
        return Err(PolicyRootError::SkillIdentityInvalid);
    }
    let hierarchy = root.join("references/instructions");
    let resolved_hierarchy = hierarchy
        .canonicalize()
        .map_err(|_| PolicyRootError::HierarchyUnavailable)?;
    if !resolved_hierarchy.is_dir() || !resolved_hierarchy.starts_with(&root) {
        return Err(PolicyRootError::HierarchyEscaped);
    }
    Ok(resolved_hierarchy)
}

pub fn valid_node(node: &str) -> bool {
    node == "." || node.split('/').all(skill_api::hierarchy::valid_segment)
}

pub fn validate_selection(paths: &[String]) -> Result<(), PathSelectionError> {
    skill_api::hierarchy::validate_selection(paths, "")
}

pub fn discover(root: &Path, node: &Path) -> Result<Vec<DiscoveredChild>, DiscoveryError> {
    skill_api::discovery::direct_children(root, node, "tags")
}

pub fn load<F, R>(
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
    tooling_feature::policy_composition::load_policy(
        root,
        selected,
        reference_applies,
        review_compatibility,
        previous,
    )
}
