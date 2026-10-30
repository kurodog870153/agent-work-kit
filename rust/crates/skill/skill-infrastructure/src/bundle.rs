//! Shared Skill bundle file enumeration, boundary checks, and content hashes.

use std::fs;
use std::path::{Path, PathBuf};

use skill_model::SkillBundleFile;
use skill_operations::canonical::{canonical_sha256, raw_sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BundleReadError {
    RootResolution(PathBuf),
    RootNotDirectory(PathBuf),
    PathResolution(String),
    PathEscapesRoot { path: String, resolved: PathBuf },
    FileRead(PathBuf),
    InvalidUtf8 { source: PathBuf, byte_offset: usize },
}

fn ignored(path: &Path) -> bool {
    path.components()
        .any(|part| part.as_os_str() == "__pycache__")
        || path.file_name().is_some_and(|name| name == ".DS_Store")
        || path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("pyc"))
}

fn collect(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), BundleReadError> {
    let entries = fs::read_dir(directory)
        .map_err(|_| BundleReadError::PathResolution(directory.to_string_lossy().into()))?;
    for entry in entries {
        let entry = entry
            .map_err(|_| BundleReadError::PathResolution(directory.to_string_lossy().into()))?;
        let path = entry.path();
        if ignored(&path) {
            continue;
        }
        if path.is_file() {
            output.push(path);
        } else if path.is_dir() {
            collect(&path, output)?;
        }
    }
    Ok(())
}

pub fn read_bundle_files(skill_root: &Path) -> Result<Vec<SkillBundleFile>, BundleReadError> {
    let root = skill_root
        .canonicalize()
        .map_err(|_| BundleReadError::RootResolution(skill_root.to_path_buf()))?;
    if !root.is_dir() {
        return Err(BundleReadError::RootNotDirectory(root));
    }
    let mut paths = vec![root.join("SKILL.md")];
    for name in ["agents", "assets", "references", "scripts"] {
        let directory = root.join(name);
        if directory.is_dir() {
            collect(&directory, &mut paths)?;
        }
    }
    paths.retain(|path| !ignored(path));
    paths.sort_by_key(|path| {
        path.strip_prefix(&root)
            .expect("collected beneath root")
            .to_string_lossy()
            .to_lowercase()
    });
    let mut files = Vec::new();
    for path in paths {
        let relative = path
            .strip_prefix(&root)
            .expect("collected beneath root")
            .to_string_lossy()
            .replace('\\', "/");
        let resolved = path
            .canonicalize()
            .map_err(|_| BundleReadError::PathResolution(relative.clone()))?;
        if !resolved.starts_with(&root) {
            return Err(BundleReadError::PathEscapesRoot {
                path: relative,
                resolved,
            });
        }
        let raw = fs::read(&resolved).map_err(|_| BundleReadError::FileRead(resolved.clone()))?;
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let is_text = [
            "bat", "command", "json", "md", "ps1", "py", "sh", "toml", "txt", "yaml", "yml",
        ]
        .contains(&extension.as_str())
            || relative == "SKILL.md";
        let normalization = if is_text { "canonical-text" } else { "raw" };
        let content_sha256 = if is_text {
            canonical_sha256(&raw).map_err(|invalid| BundleReadError::InvalidUtf8 {
                source: resolved,
                byte_offset: invalid.valid_up_to(),
            })?
        } else {
            raw_sha256(&raw)
        };
        files.push(SkillBundleFile {
            path: relative,
            normalization: normalization.into(),
            content_sha256,
        });
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "skill-bundle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn bundle_hashes_text_and_rejects_damaged_utf8() {
        let root = temporary_root();
        fs::write(root.join("SKILL.md"), b"hello\r\n").unwrap();
        let files = read_bundle_files(&root).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].normalization, "canonical-text");
        assert_eq!(
            files[0].content_sha256,
            skill_operations::canonical::raw_sha256(b"hello\n")
        );
        fs::write(root.join("SKILL.md"), b"\xff").unwrap();
        assert!(matches!(
            read_bundle_files(&root),
            Err(BundleReadError::InvalidUtf8 { byte_offset: 0, .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn bundle_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;
        let root = temporary_root();
        let outside = temporary_root();
        fs::write(root.join("SKILL.md"), b"hello\n").unwrap();
        fs::create_dir(root.join("references")).unwrap();
        fs::write(outside.join("secret.md"), b"outside\n").unwrap();
        symlink(outside.join("secret.md"), root.join("references/secret.md")).unwrap();
        assert!(matches!(read_bundle_files(&root),
            Err(BundleReadError::PathEscapesRoot { path, .. }) if path == "references/secret.md"));
    }
}
