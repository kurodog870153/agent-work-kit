//! Generic path and ancestor rules for hierarchical Skill sources.

use std::collections::{BTreeSet, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSelectionError {
    Invalid(String),
    Duplicate,
    Redundant {
        path: String,
        descendants: Vec<String>,
    },
}

pub fn valid_segment(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

pub fn validate_selection(paths: &[String], reserved_root: &str) -> Result<(), PathSelectionError> {
    for path in paths {
        if path.is_empty()
            || path
                .split('/')
                .any(|part| part == reserved_root || !valid_segment(part))
        {
            return Err(PathSelectionError::Invalid(path.clone()));
        }
    }
    let mut unique = HashSet::new();
    if paths.iter().any(|path| !unique.insert(path)) {
        return Err(PathSelectionError::Duplicate);
    }
    for path in paths {
        let prefix = format!("{path}/");
        let descendants: Vec<_> = paths
            .iter()
            .filter(|candidate| candidate.starts_with(&prefix))
            .cloned()
            .collect();
        if !descendants.is_empty() {
            return Err(PathSelectionError::Redundant {
                path: path.clone(),
                descendants,
            });
        }
    }
    Ok(())
}

pub fn ancestors(path: &str) -> Vec<String> {
    let parts: Vec<_> = path.split('/').collect();
    (1..=parts.len())
        .map(|depth| parts[..depth].join("/"))
        .collect()
}

pub fn resolved_paths(root: &str, selected: &[String]) -> Vec<String> {
    let mut resolved = vec![root.to_owned()];
    for path in selected {
        for ancestor in ancestors(path) {
            if !resolved.contains(&ancestor) {
                resolved.push(ancestor);
            }
        }
    }
    resolved
}

pub fn sorted_paths(paths: BTreeSet<String>, root: &str) -> Vec<String> {
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort_by_key(|path| (path != root, path.clone()));
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_selection_requires_valid_nonoverlapping_segments() {
        assert!(validate_selection(&["web/backend".into(), "java/ast".into()], "general").is_ok());
        assert_eq!(
            validate_selection(&["Web".into()], "general"),
            Err(PathSelectionError::Invalid("Web".into()))
        );
        assert_eq!(
            validate_selection(&["general".into()], "general"),
            Err(PathSelectionError::Invalid("general".into()))
        );
        assert_eq!(
            validate_selection(&["web".into(), "web".into()], "general"),
            Err(PathSelectionError::Duplicate)
        );
        assert!(matches!(
            validate_selection(&["web".into(), "web/backend".into()], "general"),
            Err(PathSelectionError::Redundant { .. })
        ));
    }

    #[test]
    fn ancestors_deduplicate_across_branches_and_preserve_first_visit_order() {
        assert_eq!(
            resolved_paths(
                "general",
                &["web/backend/java".into(), "web/frontend".into()]
            ),
            [
                "general",
                "web",
                "web/backend",
                "web/backend/java",
                "web/frontend"
            ]
        );
        assert_eq!(ancestors("web/backend"), ["web", "web/backend"]);
    }
}
