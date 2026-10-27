//! One-level, metadata-only hierarchy discovery.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use skill_model::SkillMetadata;
use skill_operations::hierarchy::valid_segment;

use crate::hierarchy::{MetadataReadError, parse_metadata_bytes};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryError {
    RootMissing,
    CurrentEscaped,
    CurrentMetadata(MetadataReadError),
    ListingFailed,
    InvalidChildName,
    ChildEscaped,
    ChildMetadata(MetadataReadError),
}

#[derive(Debug, Clone)]
pub struct DiscoveredChild {
    pub name: String,
    pub path: PathBuf,
    pub metadata: Result<SkillMetadata, DiscoveryError>,
}

#[derive(Debug)]
enum EntrypointError {
    Escaped,
    Unreadable,
}

fn validate_entrypoint(path: &Path, directory: &Path) -> Result<(), EntrypointError> {
    let resolved = path
        .canonicalize()
        .map_err(|_| EntrypointError::Unreadable)?;
    if !resolved.is_file()
        || resolved.parent() != Some(directory)
        || fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err(EntrypointError::Escaped);
    }
    Ok(())
}

pub fn read_metadata_only(
    path: &Path,
    tag_field: &str,
) -> Result<SkillMetadata, MetadataReadError> {
    let mut file = File::open(path).map_err(|_| MetadataReadError::FileRead)?;
    let mut header = Vec::new();
    let mut line = Vec::new();
    let mut delimiters = 0;
    loop {
        let mut byte = [0_u8];
        let count = file
            .read(&mut byte)
            .map_err(|_| MetadataReadError::FileRead)?;
        if count == 0 {
            break;
        }
        header.push(byte[0]);
        line.push(byte[0]);
        if byte[0] == b'\n' {
            let checked = line.strip_suffix(b"\n").unwrap_or(&line);
            let checked = checked.strip_suffix(b"\r").unwrap_or(checked);
            let checked = if delimiters == 0 {
                checked.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(checked)
            } else {
                checked
            };
            if checked == b"---" {
                delimiters += 1;
                if delimiters == 2 {
                    break;
                }
            }
            line.clear();
        }
        if header.len() > 1024 * 1024 {
            return Err(MetadataReadError::UnterminatedFrontmatter);
        }
    }
    parse_metadata_bytes(&header, tag_field)
}

pub fn direct_children(
    root: &Path,
    current: &Path,
    tag_field: &str,
) -> Result<Vec<DiscoveredChild>, DiscoveryError> {
    let resolved_root = root
        .canonicalize()
        .map_err(|_| DiscoveryError::RootMissing)?;
    if !resolved_root.is_dir() {
        return Err(DiscoveryError::RootMissing);
    }
    let resolved_current = current
        .canonicalize()
        .map_err(|_| DiscoveryError::CurrentEscaped)?;
    if !resolved_current.is_dir() || !resolved_current.starts_with(&resolved_root) {
        return Err(DiscoveryError::CurrentEscaped);
    }
    let entrypoint = current.join("instructions.md");
    validate_entrypoint(&entrypoint, &resolved_current).map_err(|error| match error {
        EntrypointError::Escaped => DiscoveryError::CurrentEscaped,
        EntrypointError::Unreadable => DiscoveryError::CurrentMetadata(MetadataReadError::FileRead),
    })?;
    read_metadata_only(&entrypoint, tag_field).map_err(DiscoveryError::CurrentMetadata)?;
    let mut children = Vec::new();
    for entry in fs::read_dir(current).map_err(|_| DiscoveryError::ListingFailed)? {
        let entry = entry.map_err(|_| DiscoveryError::ListingFailed)?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let metadata = if !valid_segment(&name) {
            Err(DiscoveryError::InvalidChildName)
        } else {
            let resolved_child = path
                .canonicalize()
                .map_err(|_| DiscoveryError::ChildEscaped);
            match resolved_child {
                Ok(resolved_child)
                    if resolved_child.parent() == Some(resolved_current.as_path()) =>
                {
                    let entrypoint = path.join("instructions.md");
                    match validate_entrypoint(&entrypoint, &resolved_child) {
                        Ok(()) => read_metadata_only(&entrypoint, tag_field)
                            .map_err(DiscoveryError::ChildMetadata),
                        Err(EntrypointError::Escaped) => Err(DiscoveryError::ChildEscaped),
                        Err(EntrypointError::Unreadable) => {
                            Err(DiscoveryError::ChildMetadata(MetadataReadError::FileRead))
                        }
                    }
                }
                _ => Err(DiscoveryError::ChildEscaped),
            }
        };
        children.push(DiscoveredChild {
            name,
            path,
            metadata,
        });
    }
    children.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(children)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn entrypoints_enforce_same_boundary_at_every_depth() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "skill-discovery-boundary-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root, b"root");
        node(&root.join("parent"), b"parent");
        node(&root.join("parent/child"), b"child");
        let outside = root.with_extension("outside.md");
        fs::write(&outside, fs::read(root.join("instructions.md")).unwrap()).unwrap();

        for directory in [&root, &root.join("parent")] {
            assert!(direct_children(&root, directory, "tags").is_ok());
            let entrypoint = directory.join("instructions.md");
            let original = fs::read(&entrypoint).unwrap();
            fs::remove_file(&entrypoint).unwrap();
            symlink(&outside, &entrypoint).unwrap();
            assert!(matches!(
                direct_children(&root, directory, "tags"),
                Err(DiscoveryError::CurrentEscaped)
            ));
            fs::remove_file(&entrypoint).unwrap();
            symlink(root.join("missing.md"), &entrypoint).unwrap();
            assert!(matches!(
                direct_children(&root, directory, "tags"),
                Err(DiscoveryError::CurrentMetadata(MetadataReadError::FileRead))
            ));
            fs::remove_file(&entrypoint).unwrap();
            fs::write(&entrypoint, original).unwrap();
        }

        let child = root.join("parent/child/instructions.md");
        fs::remove_file(&child).unwrap();
        symlink(&outside, &child).unwrap();
        let children = direct_children(&root, &root.join("parent"), "tags").unwrap();
        assert!(matches!(
            &children[0].metadata,
            Err(DiscoveryError::ChildEscaped)
        ));
        fs::remove_file(&child).unwrap();
        symlink(root.join("missing.md"), &child).unwrap();
        let children = direct_children(&root, &root.join("parent"), "tags").unwrap();
        assert!(matches!(
            &children[0].metadata,
            Err(DiscoveryError::ChildMetadata(MetadataReadError::FileRead))
        ));
    }

    fn node(directory: &Path, body: &[u8]) {
        fs::create_dir_all(directory).unwrap();
        let mut content = b"---\nname: Node\ndescription: Applicable node.\nmetadata:\n  tags: [tool-policy]\n---\n".to_vec();
        content.extend_from_slice(body);
        fs::write(directory.join("instructions.md"), content).unwrap();
    }

    #[test]
    fn discovery_reads_only_direct_child_headers_at_arbitrary_depth() {
        let root = std::env::temp_dir().join(format!(
            "skill-discovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root, b"root body");
        node(&root.join("capabilities"), b"selected body");
        node(&root.join("families"), b"\xff unselected body");
        node(&root.join("capabilities/deep/more"), b"deep body");
        node(&root.join("capabilities/deep"), b"parent body");
        let top = direct_children(&root, &root, "tags").unwrap();
        assert_eq!(
            top.iter()
                .map(|child| child.name.as_str())
                .collect::<Vec<_>>(),
            ["capabilities", "families"]
        );
        assert!(top.iter().all(|child| child.metadata.is_ok()));
        assert!(read_metadata_only(&root.join("families/instructions.md"), "tags").is_ok());
        assert!(matches!(
            crate::hierarchy::read_metadata(&root.join("families/instructions.md"), "tags"),
            Err(MetadataReadError::InvalidUtf8(_))
        ));
        let next = direct_children(&root, &root.join("capabilities"), "tags").unwrap();
        assert_eq!(
            next.iter()
                .map(|child| child.name.as_str())
                .collect::<Vec<_>>(),
            ["deep"]
        );
        let deepest = direct_children(&root, &root.join("capabilities/deep"), "tags").unwrap();
        assert_eq!(deepest[0].name, "more");
        assert!(
            direct_children(&root, &root.join("capabilities/deep/more"), "tags")
                .unwrap()
                .is_empty()
        );
        fs::remove_file(root.join("capabilities/deep/more/instructions.md")).unwrap();
        fs::remove_dir(root.join("capabilities/deep/more")).unwrap();
        assert!(
            direct_children(&root, &root.join("capabilities/deep"), "tags")
                .unwrap()
                .is_empty()
        );
        node(&root.join("capabilities/deep/new-child"), b"new body");
        assert_eq!(
            direct_children(&root, &root.join("capabilities/deep"), "tags").unwrap()[0].name,
            "new-child"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let outside = root.with_extension("outside");
            node(&outside, b"outside");
            symlink(&outside, root.join("outside-link")).unwrap();
            assert!(matches!(
                &direct_children(&root, &root, "tags")
                    .unwrap()
                    .iter()
                    .find(|child| child.name == "outside-link")
                    .unwrap()
                    .metadata,
                Err(DiscoveryError::ChildEscaped)
            ));
        }
        fs::write(root.join("instructions.md"), b"invalid root").unwrap();
        assert!(matches!(
            direct_children(&root, &root, "tags"),
            Err(DiscoveryError::CurrentMetadata(
                MetadataReadError::MissingFrontmatter
            ))
        ));
    }

    #[test]
    fn installed_tooling_root_exposes_only_orthogonal_dimensions() {
        let root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/tooling/references/instructions"
        ));
        let children = direct_children(root, root, "tags").unwrap();
        assert_eq!(
            children
                .iter()
                .map(|child| child.name.as_str())
                .collect::<Vec<_>>(),
            ["capabilities", "families", "interfaces"]
        );
        assert!(children.iter().all(|child| child.metadata.is_ok()));
        let capability =
            direct_children(root, &root.join("capabilities/code-analysis"), "tags").unwrap();
        assert_eq!(
            capability
                .iter()
                .map(|child| child.name.as_str())
                .collect::<Vec<_>>(),
            ["ast", "lsp"]
        );
    }
}
