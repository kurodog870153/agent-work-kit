//! Reusable Skill catalog discovery over a host-provided repository.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

#[derive(Debug, Clone)]
pub struct SkillRoot {
    pub scope: String,
    pub locator: String,
}

pub trait SkillCatalogRepository {
    type Error;

    fn roots(&self) -> Vec<SkillRoot>;
    fn list_directories(&self, root_index: usize) -> Result<Vec<PathBuf>, Self::Error>;
    fn has_entrypoint(&self, directory: &Path) -> bool;
    fn resolve_directory(&self, directory: &Path) -> Option<PathBuf>;
    fn summary(
        &self,
        root_index: usize,
        resolved: &Path,
        source: &str,
    ) -> Result<Value, Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillPathError {
    InvalidScope,
    InvalidRootLocator,
    InvalidSource,
}

pub fn validate_root(scope: &str, locator: &str) -> Result<(), SkillPathError> {
    if !["repo", "user", "admin", "system"].contains(&scope) {
        return Err(SkillPathError::InvalidScope);
    }
    if locator.is_empty()
        || locator.starts_with('/')
        || locator
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(SkillPathError::InvalidRootLocator);
    }
    Ok(())
}

pub fn validate_source(source: &str) -> Result<&str, SkillPathError> {
    let parts: Vec<_> = source.split('/').collect();
    if parts.len() != 2
        || parts[0].is_empty()
        || parts[0] == "."
        || parts[0] == ".."
        || parts[1] != "SKILL.md"
    {
        return Err(SkillPathError::InvalidSource);
    }
    Ok(parts[0])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataHeaderError {
    InvalidObject,
    InvalidIdentity,
    InvalidMetadata,
}

pub struct MetadataHeader<'a> {
    pub name: &'a str,
    pub description: &'a str,
    pub metadata: Option<&'a Map<String, Value>>,
}

pub fn metadata_header(frontmatter: &Value) -> Result<MetadataHeader<'_>, MetadataHeaderError> {
    let object = frontmatter
        .as_object()
        .ok_or(MetadataHeaderError::InvalidObject)?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let description = object
        .get("description")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    let (Some(name), Some(description)) = (name, description) else {
        return Err(MetadataHeaderError::InvalidIdentity);
    };
    let metadata = match object.get("metadata") {
        None | Some(Value::Null) => None,
        Some(Value::Object(metadata)) => Some(metadata),
        _ => return Err(MetadataHeaderError::InvalidMetadata),
    };
    Ok(MetadataHeader {
        name: name.trim(),
        description: description.trim(),
        metadata,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataListError {
    InvalidField,
    InvalidValue(Vec<String>),
}

pub fn string_list(
    value: Option<&Value>,
    allowed: Option<&[&str]>,
) -> Result<Vec<String>, MetadataListError> {
    let mut items = Vec::new();
    match value {
        None | Some(Value::Null) => {}
        Some(Value::String(text)) => items.extend(
            text.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned),
        ),
        Some(Value::Array(values)) if values.iter().all(Value::is_string) => items.extend(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned),
        ),
        _ => return Err(MetadataListError::InvalidField),
    }
    if items.iter().collect::<HashSet<_>>().len() != items.len()
        || allowed.is_some_and(|allowed| items.iter().any(|item| !allowed.contains(&item.as_str())))
    {
        return Err(MetadataListError::InvalidValue(items));
    }
    Ok(items)
}

pub fn missing_ancestor<'a>(
    paths: impl IntoIterator<Item = &'a str>,
    root: &str,
) -> Option<(String, String)> {
    let paths: std::collections::BTreeSet<_> = paths.into_iter().collect();
    for path in paths.iter().copied().filter(|path| *path != root) {
        let parts: Vec<_> = path.split('/').collect();
        for depth in 1..parts.len() {
            let ancestor = parts[..depth].join("/");
            if !paths.contains(ancestor.as_str()) {
                return Some((path.into(), ancestor));
            }
        }
    }
    None
}

pub fn hierarchy_children(
    paths: &[String],
    root: &str,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut children: std::collections::BTreeMap<String, Vec<String>> = paths
        .iter()
        .map(|path| (path.clone(), Vec::new()))
        .collect();
    for path in paths.iter().filter(|path| path.as_str() != root) {
        let (parent, child) = path
            .rsplit_once('/')
            .map_or((root, path.as_str()), |(parent, child)| (parent, child));
        children
            .get_mut(parent)
            .expect("catalog ancestors are validated")
            .push(child.into());
    }
    for names in children.values_mut() {
        names.sort();
    }
    children
}

pub fn catalog<R: SkillCatalogRepository>(
    repository: &R,
    schema: &str,
    disabled_sources: &HashSet<String>,
    excluded_names: &HashSet<String>,
    failure_details: impl Fn(&R::Error) -> (&str, &str),
) -> Result<Value, R::Error> {
    let mut skills = Vec::new();
    let mut unavailable = Vec::new();
    let mut seen_paths = HashSet::new();
    for (index, root) in repository.roots().iter().enumerate() {
        for directory in repository.list_directories(index)? {
            if !repository.has_entrypoint(&directory) {
                continue;
            }
            let source = format!(
                "{}/SKILL.md",
                directory.file_name().unwrap_or_default().to_string_lossy()
            );
            let Some(resolved) = repository.resolve_directory(&directory) else {
                unavailable.push(json!({"scope": root.scope, "root": root.locator, "source": source, "code": "skill_path_resolution_failed"}));
                continue;
            };
            if !seen_paths.insert(resolved.to_string_lossy().to_lowercase())
                || disabled_sources.contains(&source)
            {
                continue;
            }
            match repository.summary(index, &resolved, &source) {
                Ok(value) if !excluded_names.contains(value["name"].as_str().unwrap_or("")) => {
                    skills.push(value)
                }
                Ok(_) => {}
                Err(failure) => {
                    let (code, message) = failure_details(&failure);
                    unavailable.push(json!({"scope": root.scope, "root": root.locator, "source": source, "code": code, "message": message}));
                }
            }
        }
    }
    Ok(json!({"schema": schema, "skills": skills, "unavailable": unavailable}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    struct Repository {
        reads: Cell<usize>,
    }

    impl SkillCatalogRepository for Repository {
        type Error = &'static str;

        fn roots(&self) -> Vec<SkillRoot> {
            vec![SkillRoot {
                scope: "repo".into(),
                locator: "skills".into(),
            }]
        }
        fn list_directories(&self, _: usize) -> Result<Vec<PathBuf>, Self::Error> {
            Ok(["first", "disabled", "unresolved", "broken", "excluded"]
                .into_iter()
                .map(PathBuf::from)
                .collect())
        }
        fn has_entrypoint(&self, _: &Path) -> bool {
            true
        }
        fn resolve_directory(&self, directory: &Path) -> Option<PathBuf> {
            (directory != Path::new("unresolved")).then(|| directory.to_path_buf())
        }
        fn summary(&self, _: usize, _: &Path, source: &str) -> Result<Value, Self::Error> {
            self.reads.set(self.reads.get() + 1);
            if source == "broken/SKILL.md" {
                Err("invalid")
            } else {
                Ok(json!({"name":source}))
            }
        }
    }

    #[test]
    fn catalog_loads_only_selected_summaries_and_reports_failures() {
        let repository = Repository {
            reads: Cell::new(0),
        };
        let result = catalog(
            &repository,
            "skill-catalog",
            &HashSet::from(["disabled/SKILL.md".into()]),
            &HashSet::from(["excluded/SKILL.md".into()]),
            |error| (*error, "Summary failed"),
        )
        .unwrap();
        assert_eq!(repository.reads.get(), 3);
        assert_eq!(result["skills"], json!([{"name":"first/SKILL.md"}]));
        assert_eq!(
            result["unavailable"],
            json!([
                {"scope":"repo","root":"skills","source":"unresolved/SKILL.md","code":"skill_path_resolution_failed"},
                {"scope":"repo","root":"skills","source":"broken/SKILL.md","code":"invalid","message":"Summary failed"}
            ])
        );
    }

    #[test]
    fn root_and_source_paths_reject_escape_and_invalid_scope() {
        assert!(validate_root("repo", "skills/work").is_ok());
        assert_eq!(
            validate_root("guest", "skills"),
            Err(SkillPathError::InvalidScope)
        );
        assert_eq!(
            validate_root("repo", "../skills"),
            Err(SkillPathError::InvalidRootLocator)
        );
        assert_eq!(validate_source("example/SKILL.md"), Ok("example"));
        for source in ["../SKILL.md", "example/other.md", "example/nested/SKILL.md"] {
            assert_eq!(validate_source(source), Err(SkillPathError::InvalidSource));
        }
    }

    #[test]
    fn metadata_header_and_lists_reject_invalid_or_duplicate_values() {
        let value = json!({"name":" example ","description":" Read code ","metadata":{"tags":["analysis"]}});
        let header = metadata_header(&value).unwrap();
        assert_eq!((header.name, header.description), ("example", "Read code"));
        assert_eq!(
            string_list(header.metadata.and_then(|value| value.get("tags")), None).unwrap(),
            ["analysis"]
        );
        assert!(matches!(
            metadata_header(&json!({"name":"x","description":"y","metadata":[]})),
            Err(MetadataHeaderError::InvalidMetadata)
        ));
        assert_eq!(
            string_list(Some(&json!(["analysis", "analysis"])), None),
            Err(MetadataListError::InvalidValue(vec![
                "analysis".into(),
                "analysis".into()
            ]))
        );
        assert_eq!(
            string_list(Some(&json!([1])), None),
            Err(MetadataListError::InvalidField)
        );
    }

    #[test]
    fn hierarchy_catalog_requires_ancestors_and_builds_immediate_children() {
        let paths = ["general", "web", "web/backend"];
        assert_eq!(missing_ancestor(paths, "general"), None);
        let children = hierarchy_children(&paths.map(str::to_owned), "general");
        assert_eq!(children["general"], ["web"]);
        assert_eq!(children["web"], ["backend"]);
        assert_eq!(
            missing_ancestor(["general", "web/backend"], "general"),
            Some(("web/backend".into(), "web".into()))
        );
    }
}
