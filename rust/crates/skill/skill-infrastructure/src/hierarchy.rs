//! Shared hierarchy entrypoint metadata reader.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use skill_model::{SkillMetadata, instruction::InstructionFrontmatter};
use skill_operations::hierarchy::valid_segment;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataReadError {
    FileRead,
    InvalidUtf8(usize),
    MissingFrontmatter,
    UnterminatedFrontmatter,
    InvalidYaml,
    InvalidObject,
    InvalidFields,
    InvalidText(&'static str),
    InvalidMetadata,
    InvalidMetadataFields,
    InvalidTags,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HierarchyScanError {
    Unreadable(PathBuf),
    Escaped(PathBuf),
    InvalidPath(String),
    NestedRoot(String),
}

pub fn scan_entrypoints(
    mode_dir: &Path,
    resolved_mode: &Path,
    reserved_root: &str,
) -> Result<Vec<(String, PathBuf)>, HierarchyScanError> {
    let mut entries = Vec::new();
    visit(
        mode_dir,
        mode_dir,
        resolved_mode,
        reserved_root,
        &mut entries,
    )?;
    Ok(entries)
}

fn visit(
    current: &Path,
    mode_dir: &Path,
    resolved_mode: &Path,
    reserved_root: &str,
    output: &mut Vec<(String, PathBuf)>,
) -> Result<(), HierarchyScanError> {
    let entries =
        fs::read_dir(current).map_err(|_| HierarchyScanError::Unreadable(current.to_path_buf()))?;
    for entry in entries {
        let entry = entry.map_err(|_| HierarchyScanError::Unreadable(current.to_path_buf()))?;
        let path = entry.path();
        if path.is_dir() {
            visit(&path, mode_dir, resolved_mode, reserved_root, output)?;
            continue;
        }
        if path.file_name().and_then(|name| name.to_str()) != Some("instructions.md") {
            continue;
        }
        let resolved = path
            .canonicalize()
            .map_err(|_| HierarchyScanError::Unreadable(path.clone()))?;
        if !resolved.is_file() || !resolved.starts_with(resolved_mode) {
            return Err(HierarchyScanError::Escaped(path));
        }
        let relative = path
            .parent()
            .unwrap()
            .strip_prefix(mode_dir)
            .expect("visited under mode directory");
        let parts: Vec<_> = relative
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect();
        let hierarchy_path = parts.join("/");
        if parts.is_empty() || parts.iter().any(|part| !valid_segment(part)) {
            return Err(HierarchyScanError::InvalidPath(hierarchy_path));
        }
        if parts.iter().any(|part| part == reserved_root) && hierarchy_path != reserved_root {
            return Err(HierarchyScanError::NestedRoot(hierarchy_path));
        }
        output.push((hierarchy_path, path));
    }
    Ok(())
}

pub fn read_metadata(path: &Path, tag_field: &str) -> Result<SkillMetadata, MetadataReadError> {
    let raw = fs::read(path).map_err(|_| MetadataReadError::FileRead)?;
    parse_metadata_bytes(&raw, tag_field)
}

pub fn parse_metadata_bytes(
    raw: &[u8],
    tag_field: &str,
) -> Result<SkillMetadata, MetadataReadError> {
    if tag_field != "tags" {
        return Err(MetadataReadError::InvalidMetadataFields);
    }
    let text = std::str::from_utf8(raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(raw))
        .map_err(|invalid| MetadataReadError::InvalidUtf8(invalid.valid_up_to()))?;
    let lines: Vec<_> = text.lines().collect();
    if lines.first() != Some(&"---") {
        return Err(MetadataReadError::MissingFrontmatter);
    }
    let end = lines
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, line)| **line == "---")
        .map(|(index, _)| index)
        .ok_or(MetadataReadError::UnterminatedFrontmatter)?;
    let loaded: Value = crate::strict_yaml::parse(&lines[1..end].join("\n"))
        .map_err(|_| MetadataReadError::InvalidYaml)?;
    let object = loaded.as_object().ok_or(MetadataReadError::InvalidObject)?;
    if object.len() != 3
        || !["name", "description", "metadata"]
            .iter()
            .all(|key| object.contains_key(*key))
    {
        return Err(MetadataReadError::InvalidFields);
    }
    let name = object["name"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or(MetadataReadError::InvalidText("name"))?
        .trim();
    let description = object["description"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or(MetadataReadError::InvalidText("description"))?
        .trim();
    let metadata = object["metadata"]
        .as_object()
        .ok_or(MetadataReadError::InvalidMetadata)?;
    if metadata.len() != 1 || !metadata.contains_key(tag_field) {
        return Err(MetadataReadError::InvalidMetadataFields);
    }
    let tags = metadata[tag_field]
        .as_array()
        .filter(|tags| {
            !tags.is_empty()
                && tags
                    .iter()
                    .all(|tag| tag.as_str().is_some_and(valid_segment))
        })
        .ok_or(MetadataReadError::InvalidTags)?;
    let unique: BTreeSet<_> = tags.iter().map(|tag| tag.as_str().unwrap()).collect();
    if unique.len() != tags.len() {
        return Err(MetadataReadError::InvalidTags);
    }
    let name = name.to_owned();
    let description = description.to_owned();
    let mut frontmatter: InstructionFrontmatter =
        serde_json::from_value(loaded).expect("validated instruction frontmatter shape");
    frontmatter.name = name;
    frontmatter.description = description;
    Ok(frontmatter.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    use skill_model::instruction::InstructionFrontmatter;

    #[test]
    fn instruction_frontmatter_template_matches_reader_and_model() {
        let template = include_str!("../../skill-model/resources/instruction-frontmatter.yaml");
        let value = crate::strict_yaml::parse(template).unwrap();
        let typed: InstructionFrontmatter = serde_json::from_value(value).unwrap();
        let content = format!("---\n{template}---\n# Example\n");
        let parsed = parse_metadata_bytes(content.as_bytes(), "tags").unwrap();
        assert_eq!(parsed.name, typed.name);
        assert_eq!(parsed.description, typed.description);
        assert_eq!(parsed.tags, typed.metadata.tags);
    }

    #[test]
    fn entrypoint_scan_checks_paths_and_symlink_boundary() {
        let root = std::env::temp_dir().join(format!(
            "skill-entrypoints-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mode = root.join("mode");
        fs::create_dir_all(mode.join("general")).unwrap();
        fs::create_dir_all(mode.join("analysis/deep")).unwrap();
        fs::write(mode.join("general/instructions.md"), "root").unwrap();
        fs::write(mode.join("analysis/instructions.md"), "parent").unwrap();
        fs::write(mode.join("analysis/deep/instructions.md"), "child").unwrap();
        let resolved = mode.canonicalize().unwrap();
        let found = scan_entrypoints(&mode, &resolved, "general").unwrap();
        assert_eq!(found.len(), 3);
        assert!(found.iter().any(|(path, _)| path == "analysis/deep"));
        fs::create_dir_all(mode.join("bad_name")).unwrap();
        fs::write(mode.join("bad_name/instructions.md"), "invalid").unwrap();
        assert_eq!(
            scan_entrypoints(&mode, &resolved, "general"),
            Err(HierarchyScanError::InvalidPath("bad_name".into()))
        );
        fs::remove_file(mode.join("bad_name/instructions.md")).unwrap();
        fs::create_dir_all(mode.join("analysis/general")).unwrap();
        fs::write(mode.join("analysis/general/instructions.md"), "nested").unwrap();
        assert_eq!(
            scan_entrypoints(&mode, &resolved, "general"),
            Err(HierarchyScanError::NestedRoot("analysis/general".into()))
        );
        fs::remove_file(mode.join("analysis/general/instructions.md")).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = root.join("outside.md");
            fs::write(&outside, "outside").unwrap();
            symlink(&outside, mode.join("analysis/deep/outside.md")).unwrap();
            assert_eq!(
                scan_entrypoints(&mode, &resolved, "general").unwrap().len(),
                3
            );
            symlink(&outside, mode.join("analysis/deep/instructions.md.link")).unwrap();
            fs::remove_file(mode.join("analysis/deep/instructions.md")).unwrap();
            symlink(&outside, mode.join("analysis/deep/instructions.md")).unwrap();
            assert_eq!(
                scan_entrypoints(&mode, &resolved, "general"),
                Err(HierarchyScanError::Escaped(
                    mode.join("analysis/deep/instructions.md")
                ))
            );
        }
    }

    #[test]
    fn hierarchy_metadata_validates_schema_tags_and_utf8() {
        let root = std::env::temp_dir().join(format!(
            "skill-hierarchy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let path = root.join("instructions.md");
        fs::write(&path, b"---\nname: Example\ndescription: Read code.\nmetadata:\n  tags: [analysis]\n---\nBody.\n").unwrap();
        assert_eq!(read_metadata(&path, "tags").unwrap().tags, ["analysis"]);
        assert_eq!(
            read_metadata(&path, "work-tags"),
            Err(MetadataReadError::InvalidMetadataFields)
        );
        fs::write(&path, b"---\nname: Example\ndescription: Read code.\nmetadata:\n  work-tags: [analysis]\n---\nBody.\n").unwrap();
        assert_eq!(
            read_metadata(&path, "tags"),
            Err(MetadataReadError::InvalidMetadataFields)
        );
        fs::write(&path, b"---\nname: Example\ndescription: Read code.\nmetadata:\n  tags: [analysis, analysis]\n---\n").unwrap();
        assert_eq!(
            read_metadata(&path, "tags"),
            Err(MetadataReadError::InvalidTags)
        );
        fs::write(&path, b"---\nname: \xff\n---\n").unwrap();
        assert_eq!(
            read_metadata(&path, "tags"),
            Err(MetadataReadError::InvalidUtf8(10))
        );
        fs::write(&path, b"\xef\xbb\xbf---\nname: Example\ndescription: Read code.\nmetadata:\n  tags: [analysis]\n---\n").unwrap();
        assert_eq!(read_metadata(&path, "tags").unwrap().tags, ["analysis"]);
        fs::write(&path, b"\xef\xbb\xbf\xef\xbb\xbf---\nname: Example\ndescription: Read code.\nmetadata:\n  tags: [analysis]\n---\n").unwrap();
        assert_eq!(
            read_metadata(&path, "tags"),
            Err(MetadataReadError::MissingFrontmatter)
        );
        fs::write(&path, b"---\nname: One\nname: Two\ndescription: Read code.\nmetadata:\n  tags: [analysis]\n---\n").unwrap();
        assert_eq!(
            read_metadata(&path, "tags"),
            Err(MetadataReadError::InvalidYaml)
        );
    }

    #[test]
    fn installed_work_hierarchy_files_use_common_metadata_schema() {
        let root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/work/references/instructions"
        ));
        let mut pending = vec![root.to_path_buf()];
        let mut count = 0;
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else if path.extension().is_some_and(|extension| extension == "md") {
                    read_metadata(&path, "tags")
                        .unwrap_or_else(|failure| panic!("{}: {failure:?}", path.display()));
                    count += 1;
                }
            }
        }
        assert_eq!(count, 37);
    }

    #[test]
    fn tooling_skeleton_has_valid_root_and_complete_ancestors() {
        let root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/tooling/references/instructions"
        ));
        let paths = [
            "capabilities",
            "capabilities/code-analysis",
            "capabilities/code-analysis/lsp",
            "capabilities/code-analysis/ast",
            "interfaces",
            "interfaces/mcp",
            "families",
            "families/serena",
            "families/tree-sitter",
        ];
        read_metadata(&root.join("instructions.md"), "tags").unwrap();
        assert!(
            crate::bootstrap::reference_routes(
                &fs::read_to_string(root.join("instructions.md")).unwrap()
            )
            .unwrap()
            .is_empty()
        );
        let known: BTreeSet<_> = paths.iter().copied().collect();
        for path in paths {
            let file = root.join(path).join("instructions.md");
            read_metadata(&file, "tags").unwrap();
            let mut parts = path.split('/').collect::<Vec<_>>();
            while parts.len() > 1 {
                parts.pop();
                assert!(known.contains(parts.join("/").as_str()));
            }
        }
        let missing = std::env::temp_dir().join(format!(
            "tooling-root-missing-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert_eq!(
            read_metadata(&missing.join("instructions.md"), "tags"),
            Err(MetadataReadError::FileRead)
        );
        fs::create_dir_all(&missing).unwrap();
        fs::write(missing.join("instructions.md"), b"# Invalid root\n").unwrap();
        assert_eq!(
            read_metadata(&missing.join("instructions.md"), "tags"),
            Err(MetadataReadError::MissingFrontmatter)
        );
    }
}
