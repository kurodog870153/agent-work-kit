//! Load only selected hierarchy sources, their ancestors and applicable references.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use skill_operations::canonical::{canonical_text, raw_sha256};
use skill_operations::hierarchy::valid_segment;

use crate::bootstrap::{ReferenceRouteError, reference_routes, validate_reference_path};
use crate::hierarchy::{MetadataReadError, parse_metadata_bytes};
use crate::source::{SourceLoadError, strip_single_bom, validate_reference_metadata};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HierarchySourceError {
    InvalidSelection(String),
    Missing(PathBuf),
    Escaped(PathBuf),
    InvalidUtf8(PathBuf),
    InvalidBom(PathBuf),
    Metadata(PathBuf, MetadataReadError),
    References(PathBuf, ReferenceRouteError),
    ReferenceMetadata(PathBuf, SourceLoadError),
    MandatoryConflict(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HierarchySource {
    pub relative_path: String,
    pub raw_sha256: String,
    pub canonical_sha256: String,
    pub canonical_content: Vec<u8>,
}

pub fn load_selected<F, R>(
    root: &Path,
    selected: &[&str],
    reference_applies: F,
    review_compatibility: R,
) -> Result<Vec<HierarchySource>, HierarchySourceError>
where
    F: Fn(&str, &str, &str) -> bool,
    R: Fn(&[HierarchySource]) -> Result<(), String>,
{
    let resolved_root = root
        .canonicalize()
        .map_err(|_| HierarchySourceError::Missing(root.to_path_buf()))?;
    if !resolved_root.is_dir() {
        return Err(HierarchySourceError::Missing(root.to_path_buf()));
    }
    let mut nodes = vec![String::new()];
    let mut seen = HashSet::new();
    seen.insert(String::new());
    for selection in selected {
        if selection.is_empty() || selection.split('/').any(|part| !valid_segment(part)) {
            return Err(HierarchySourceError::InvalidSelection((*selection).into()));
        }
        let mut path = String::new();
        for part in selection.split('/') {
            if !path.is_empty() {
                path.push('/');
            }
            path.push_str(part);
            if seen.insert(path.clone()) {
                nodes.push(path.clone());
            }
        }
    }

    let mut sources = Vec::new();
    for node in nodes {
        let directory = if node.is_empty() {
            resolved_root.clone()
        } else {
            resolved_root.join(&node)
        };
        let resolved_directory = directory
            .canonicalize()
            .map_err(|_| HierarchySourceError::Missing(directory.clone()))?;
        if resolved_directory != directory || !resolved_directory.is_dir() {
            return Err(HierarchySourceError::Escaped(directory));
        }
        let entrypoint = directory.join("instructions.md");
        let entry = read_source(&entrypoint, &resolved_directory, &resolved_root)?;
        parse_metadata_bytes(&entry.canonical_content, "tags")
            .map_err(|error| HierarchySourceError::Metadata(entrypoint.clone(), error))?;
        let text = std::str::from_utf8(&entry.canonical_content)
            .expect("read_source returns UTF-8 content");
        let routes = reference_routes(text)
            .map_err(|error| HierarchySourceError::References(entrypoint.clone(), error))?;
        sources.push(entry);
        for (name, condition) in routes {
            if !reference_applies(&node, &name, &condition) {
                continue;
            }
            let reference = validate_reference_path(&directory, &name)
                .map_err(|error| HierarchySourceError::References(entrypoint.clone(), error))?;
            let reference_directory = directory.join("references");
            let source = read_source(&reference, &reference_directory, &resolved_root)?;
            validate_reference_metadata(&source.canonical_content, &name, "tags")
                .map_err(|error| HierarchySourceError::ReferenceMetadata(reference, error))?;
            sources.push(source);
        }
    }
    review_compatibility(&sources).map_err(HierarchySourceError::MandatoryConflict)?;
    Ok(sources)
}

fn read_source(
    path: &Path,
    expected_parent: &Path,
    root: &Path,
) -> Result<HierarchySource, HierarchySourceError> {
    let resolved = path
        .canonicalize()
        .map_err(|_| HierarchySourceError::Missing(path.to_path_buf()))?;
    if !resolved.is_file()
        || !resolved.starts_with(root)
        || resolved.parent() != Some(expected_parent)
        || fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(HierarchySourceError::Escaped(path.to_path_buf()));
    }
    let raw = fs::read(path).map_err(|_| HierarchySourceError::Missing(path.to_path_buf()))?;
    let without_bom =
        strip_single_bom(&raw).map_err(|_| HierarchySourceError::InvalidBom(path.to_path_buf()))?;
    let text = std::str::from_utf8(without_bom)
        .map_err(|_| HierarchySourceError::InvalidUtf8(path.to_path_buf()))?;
    let canonical_content = canonical_text(text).into_bytes();
    Ok(HierarchySource {
        relative_path: path
            .strip_prefix(root)
            .expect("validated under root")
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        raw_sha256: raw_sha256(&raw),
        canonical_sha256: raw_sha256(&canonical_content),
        canonical_content,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(directory: &Path, body: &str) {
        fs::create_dir_all(directory).unwrap();
        fs::write(
            directory.join("instructions.md"),
            format!("---\nname: Node\ndescription: Valid node.\nmetadata:\n  tags:\n    - node\n---\n{body}"),
        )
        .unwrap();
    }

    #[test]
    fn selected_sources_load_ancestors_and_applicable_references_only() {
        let root = std::env::temp_dir().join(format!(
            "skill-selected-sources-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root, "Root.\n");
        node(&root.join("capabilities"), "Parent.\n");
        node(
            &root.join("capabilities/code-analysis"),
            "Analysis.\n\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/needed.md` | Need it. |\n| `references/unused.md` | Do not need it. |\n",
        );
        node(&root.join("families"), "Unselected.\n");
        let reference_dir = root.join("capabilities/code-analysis/references");
        fs::create_dir(&reference_dir).unwrap();
        fs::write(reference_dir.join("needed.md"), "---\nname: Needed\ndescription: Valid reference.\nmetadata:\n  tags:\n    - node\n---\nRelevant.\n").unwrap();
        fs::write(reference_dir.join("unused.md"), b"\xff").unwrap();
        let loaded = load_selected(
            &root,
            &["capabilities/code-analysis"],
            |_, name, _| name == "needed",
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(loaded.len(), 4);
        assert!(loaded[0].relative_path.ends_with("instructions.md"));
        assert!(loaded[3].relative_path.ends_with("references/needed.md"));
        assert_eq!(
            loaded[3].raw_sha256,
            raw_sha256(&fs::read(reference_dir.join("needed.md")).unwrap())
        );
        assert_eq!(
            loaded[3].canonical_sha256,
            raw_sha256(&loaded[3].canonical_content)
        );
        assert!(
            !loaded
                .iter()
                .any(|source| source.relative_path.contains("families"))
        );
        assert!(matches!(
            load_selected(
                &root,
                &["capabilities/code-analysis"],
                |_, _, _| true,
                |_| Ok(())
            ),
            Err(HierarchySourceError::InvalidUtf8(_))
        ));
        assert!(matches!(
            load_selected(
                &root,
                &["capabilities/missing"],
                |_, _, _| false,
                |_| Ok(())
            ),
            Err(HierarchySourceError::Missing(_))
        ));
        assert!(matches!(
            load_selected(&root, &["../families"], |_, _, _| false, |_| Ok(())),
            Err(HierarchySourceError::InvalidSelection(_))
        ));

        node(
            &root.join("capabilities/code-analysis"),
            "Analysis.\n\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/needed.md` | Need it. |\n| `references/needed.md` | Again. |\n",
        );
        assert!(matches!(
            load_selected(
                &root,
                &["capabilities/code-analysis"],
                |_, _, _| true,
                |_| Ok(())
            ),
            Err(HierarchySourceError::References(
                _,
                ReferenceRouteError::Duplicate(_)
            ))
        ));
        node(
            &root.join("capabilities/code-analysis"),
            "Analysis.\n\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `references/escaped.md` | Need it. |\n",
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = root.join("outside.md");
            fs::write(&outside, "outside").unwrap();
            symlink(&outside, reference_dir.join("escaped.md")).unwrap();
            assert!(matches!(
                load_selected(
                    &root,
                    &["capabilities/code-analysis"],
                    |_, _, _| true,
                    |_| Ok(())
                ),
                Err(HierarchySourceError::References(
                    _,
                    ReferenceRouteError::Escaped(_)
                ))
            ));
        }
        fs::write(
            root.join("instructions.md"),
            b"\xef\xbb\xbf\xef\xbb\xbf---\n",
        )
        .unwrap();
        assert!(matches!(
            load_selected(&root, &["capabilities"], |_, _, _| false, |_| Ok(())),
            Err(HierarchySourceError::InvalidBom(_))
        ));
    }

    #[test]
    fn installed_tooling_branches_share_one_root_and_keep_source_hashes() {
        let default_root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/tooling/references/instructions"
        ));
        let installed_root = std::env::var_os("TOOLING_SKILL_TEST_ROOT");
        let root = installed_root
            .as_deref()
            .map(|root| Path::new(root).join("references/instructions"));
        let root = root.as_deref().unwrap_or(default_root);
        let loaded = load_selected(
            root,
            &[
                "capabilities/code-analysis/lsp",
                "interfaces/mcp",
                "families/serena",
            ],
            |_, _, _| false,
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(loaded.len(), 8);
        assert_eq!(loaded[0].relative_path, "instructions.md");
        let unique: HashSet<_> = loaded.iter().map(|source| &source.relative_path).collect();
        assert_eq!(unique.len(), loaded.len());
        for source in loaded {
            assert_eq!(
                source.raw_sha256,
                raw_sha256(&fs::read(root.join(source.relative_path)).unwrap())
            );
        }
    }

    #[test]
    fn host_semantic_review_blocks_conflicting_mandatory_branches() {
        let root = std::env::temp_dir().join(format!(
            "skill-policy-conflict-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root, "Root.\n");
        node(&root.join("interfaces"), "[Required] Never log tokens.\n");
        node(&root.join("families"), "[Required] Log all tokens.\n");
        let review = |sources: &[HierarchySource]| {
            let combined = sources
                .iter()
                .map(|source| String::from_utf8_lossy(&source.canonical_content))
                .collect::<Vec<_>>()
                .join("\n");
            if combined.contains("Never log tokens") && combined.contains("Log all tokens") {
                Err("incompatible token logging requirements".into())
            } else {
                Ok(())
            }
        };
        assert!(load_selected(&root, &["interfaces"], |_, _, _| false, review).is_ok());
        assert!(matches!(
            load_selected(&root, &["interfaces", "families"], |_, _, _| false, review),
            Err(HierarchySourceError::MandatoryConflict(_))
        ));
    }
}
