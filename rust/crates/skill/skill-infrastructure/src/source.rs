//! Shared instruction source file loading and legacy reference metadata checks.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use skill_model::instruction::LoadedSource;
use skill_operations::canonical::canonical_text;
use skill_operations::hierarchy::valid_segment;
use skill_operations::instruction::source_summary;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLoadError {
    Missing,
    NotFile,
    EscapesRoot(PathBuf),
    EscapesBoundary(PathBuf),
    ReadFailed,
    InvalidUtf8(usize),
    InvalidBom,
    ReferenceMetadata(&'static str),
}

pub fn load_file(
    root: &Path,
    path: &Path,
    kind: &str,
    logical_name: &str,
    boundary: Option<&Path>,
    reference_tag_field: &str,
) -> Result<LoadedSource, SourceLoadError> {
    let resolved = path.canonicalize().map_err(|_| SourceLoadError::Missing)?;
    if !resolved.is_file() {
        return Err(SourceLoadError::NotFile);
    }
    if !resolved.starts_with(root) {
        return Err(SourceLoadError::EscapesRoot(resolved));
    }
    if boundary.is_some_and(|boundary| !resolved.starts_with(boundary)) {
        return Err(SourceLoadError::EscapesBoundary(resolved));
    }
    let raw = fs::read(path).map_err(|_| SourceLoadError::ReadFailed)?;
    let bytes = strip_single_bom(&raw)?;
    let text = std::str::from_utf8(bytes)
        .map_err(|invalid| SourceLoadError::InvalidUtf8(invalid.valid_up_to()))?;
    let canonical_content = canonical_text(text).into_bytes();
    if kind == "reference" {
        if !canonical_content.starts_with(b"---\n") {
            return Err(SourceLoadError::ReferenceMetadata("missing"));
        }
        validate_reference_metadata(&canonical_content, logical_name, reference_tag_field)?;
    }
    Ok(LoadedSource {
        summary: source_summary(kind, logical_name, &canonical_content),
        canonical_content,
    })
}

pub fn strip_single_bom(raw: &[u8]) -> Result<&[u8], SourceLoadError> {
    let bytes = raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw);
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Err(SourceLoadError::InvalidBom);
    }
    Ok(bytes)
}

pub fn validate_reference_metadata(
    content: &[u8],
    _logical_name: &str,
    tag_field: &str,
) -> Result<(), SourceLoadError> {
    let text = std::str::from_utf8(content).expect("canonical content is UTF-8");
    let lines: Vec<_> = text.lines().collect();
    let end = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| **line == "---")
        .map(|(index, _)| index)
        .ok_or(SourceLoadError::ReferenceMetadata("unterminated"))?;
    let value: serde_json::Value = crate::strict_yaml::parse(&lines[1..end].join("\n"))
        .map_err(|_| SourceLoadError::ReferenceMetadata("invalid_yaml"))?;
    let fields = value
        .as_object()
        .ok_or(SourceLoadError::ReferenceMetadata("invalid_object"))?;
    let valid_fields = fields.len() == 3
        && ["name", "description", "metadata"]
            .iter()
            .all(|field| fields.contains_key(*field));
    let valid_strings = ["name", "description"].iter().all(|field| {
        fields[*field]
            .as_str()
            .is_some_and(|value| !value.trim().is_empty())
    });
    let valid_tags = fields["metadata"].as_object().is_some_and(|metadata| {
        metadata.len() == 1
            && metadata
                .get(tag_field)
                .and_then(|tags| tags.as_array())
                .is_some_and(|tags| {
                    !tags.is_empty()
                        && tags
                            .iter()
                            .all(|tag| tag.as_str().is_some_and(valid_segment))
                        && tags
                            .iter()
                            .map(|tag| tag.as_str().unwrap())
                            .collect::<HashSet<_>>()
                            .len()
                            == tags.len()
                })
    });
    if !(valid_fields && valid_strings && valid_tags) {
        return Err(SourceLoadError::ReferenceMetadata("invalid_fields"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_loading_checks_normalization_metadata_and_boundary() {
        let root = std::env::temp_dir().join(format!(
            "skill-source-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let branch = root.join("branch");
        fs::create_dir_all(&branch).unwrap();
        let resolved_root = root.canonicalize().unwrap();
        let path = branch.join("reference.md");
        fs::write(&path, b"\xef\xbb\xbfhello\r\n").unwrap();
        let loaded = load_file(
            &resolved_root,
            &path,
            "instruction",
            "branch.reference",
            None,
            "tags",
        )
        .unwrap();
        assert_eq!(loaded.canonical_content, b"hello\n");
        assert!(matches!(
            load_file(
                &resolved_root,
                &path,
                "reference",
                "branch.reference",
                None,
                "tags"
            ),
            Err(SourceLoadError::ReferenceMetadata("missing"))
        ));
        fs::write(
            &path,
            b"---\nname: Guide\ndescription: Read.\nmetadata:\n  tags: [guide]\n---\n",
        )
        .unwrap();
        assert!(
            load_file(
                &resolved_root,
                &path,
                "reference",
                "branch.reference",
                None,
                "tags"
            )
            .is_ok()
        );
        assert!(load_file(&resolved_root, &path, "reference", "wrong", None, "tags").is_ok());
        fs::write(&path, b"---\nname: Guide\ndescription: Read.\nreference-name: legacy\nmetadata:\n  tags: [guide]\n---\n").unwrap();
        assert!(matches!(
            load_file(
                &resolved_root,
                &path,
                "reference",
                "branch.reference",
                None,
                "tags"
            ),
            Err(SourceLoadError::ReferenceMetadata("invalid_fields"))
        ));
        fs::write(&path, b"---\nname: Guide\nname: Duplicate\ndescription: Read.\nmetadata:\n  tags: [guide]\n---\n").unwrap();
        assert!(matches!(
            load_file(
                &resolved_root,
                &path,
                "reference",
                "branch.reference",
                None,
                "tags"
            ),
            Err(SourceLoadError::ReferenceMetadata("invalid_yaml"))
        ));
        fs::write(
            &path,
            b"---\nname: Guide\ndescription: Read.\nmetadata:\n  tags: [guide]\n---\n",
        )
        .unwrap();
        let other = root.join("other");
        fs::create_dir(&other).unwrap();
        assert!(matches!(
            load_file(
                &resolved_root,
                &path,
                "reference",
                "branch.reference",
                Some(&other.canonicalize().unwrap()),
                "tags"
            ),
            Err(SourceLoadError::EscapesBoundary(_))
        ));
        fs::write(&path, b"\xff").unwrap();
        assert!(matches!(
            load_file(&resolved_root, &path, "instruction", "branch", None, "tags"),
            Err(SourceLoadError::InvalidUtf8(0))
        ));
        fs::write(&path, b"\xef\xbb\xbf\xef\xbb\xbf---\n").unwrap();
        assert!(matches!(
            load_file(&resolved_root, &path, "instruction", "branch", None, "tags"),
            Err(SourceLoadError::InvalidBom)
        ));
    }
}
