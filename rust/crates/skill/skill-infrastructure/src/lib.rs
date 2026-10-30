//! Filesystem discovery and metadata loading for Skill catalogs.

pub mod bootstrap;
pub mod bundle;
pub mod discovery;
pub mod hierarchy;
pub mod hierarchy_sources;
pub mod source;
pub mod strict_yaml;

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrontmatterError {
    ReadFailed,
    Missing,
    Unterminated,
    Invalid,
}

pub fn read_frontmatter(path: &Path) -> Result<Value, FrontmatterError> {
    let raw = fs::read(path).map_err(|_| FrontmatterError::ReadFailed)?;
    let raw = raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&raw);
    let lines = raw.split(|byte| *byte == b'\n').collect::<Vec<_>>();
    let delimiter = |line: &[u8]| line.strip_suffix(b"\r").unwrap_or(line) == b"---";
    if !lines.first().is_some_and(|line| delimiter(line)) {
        return Err(FrontmatterError::Missing);
    }
    let end = lines
        .iter()
        .skip(1)
        .position(|line| delimiter(line))
        .ok_or(FrontmatterError::Unterminated)?
        + 1;
    let frontmatter = lines[1..end].join(&b'\n');
    let text = std::str::from_utf8(&frontmatter).map_err(|_| FrontmatterError::Invalid)?;
    serde_yaml_ng::from_str(text).map_err(|_| FrontmatterError::Invalid)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataFileError {
    ReadFailed,
    Invalid,
}

pub fn read_metadata_file(path: &Path) -> Result<Value, MetadataFileError> {
    let raw = fs::read(path).map_err(|_| MetadataFileError::ReadFailed)?;
    let text = std::str::from_utf8(raw.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&raw))
        .map_err(|_| MetadataFileError::Invalid)?;
    serde_yaml_ng::from_str(text).map_err(|_| MetadataFileError::Invalid)
}

pub fn list_directories(path: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut directories: Vec<_> = fs::read_dir(path)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    directories.sort_by_key(|path| {
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    });
    Ok(directories)
}

pub fn has_entrypoint(directory: &Path) -> bool {
    directory.join("SKILL.md").is_file()
}

pub fn resolve_directory(directory: &Path) -> Option<PathBuf> {
    directory.canonicalize().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_accepts_bom_and_body_bytes_but_rejects_invalid_header() {
        let root = std::env::temp_dir().join(format!(
            "skill-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let valid = root.join("valid.md");
        fs::write(&valid, b"\xef\xbb\xbf---\nname: example\n---\n\xff").unwrap();
        assert_eq!(read_frontmatter(&valid).unwrap()["name"], "example");
        let invalid = root.join("invalid.md");
        fs::write(&invalid, b"---\nname: \xff\n---\n").unwrap();
        assert_eq!(read_frontmatter(&invalid), Err(FrontmatterError::Invalid));
    }
}
