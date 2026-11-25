//! Skill domain technical facts: a file-system `SourceStore`.
//!
//! Managed paths are checked without following links, then normalized and
//! constrained to their owning Shared or Skill boundary.

use std::fs;
use std::path::{Path, PathBuf};

use skill_feature::{DirEntry, SourceContent, SourceStore};
use skill_model::{Reason, SkillError, SourceId, SourceRef, SourceRoot};
use skill_operations::decode;
use skill_operations::path::{is_kebab, validate_relative};

/// Directory name of the single Shared boundary inside an installation.
pub const SHARED_DIR: &str = "shared";

/// One installation: `<install>/shared` plus one directory per Skill.
#[derive(Debug, Clone)]
pub struct FsStore {
    install: PathBuf,
}

impl FsStore {
    /// Open an installation root. Shared and Skill directories must be real
    /// directories whose real location is directly inside this installation;
    /// anything else is a mixed installation.
    pub fn open(install: impl AsRef<Path>) -> Result<Self, SkillError> {
        reject_link(install.as_ref(), Reason::MixedInstall)?;
        let install = fs::canonicalize(install.as_ref()).map_err(|error| {
            SkillError::new(Reason::Unreadable, format!("installation root: {error}"))
        })?;
        let store = Self { install };
        store.verify_boundaries()?;
        Ok(store)
    }

    fn verify_boundaries(&self) -> Result<(), SkillError> {
        let entries = fs::read_dir(&self.install)
            .map_err(|error| SkillError::new(Reason::Unreadable, error.to_string()))?;
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name != SHARED_DIR && !is_kebab(&name) {
                continue;
            }
            reject_link(&entry.path(), Reason::MixedInstall)?;
            let Ok(real) = fs::canonicalize(entry.path()) else {
                continue;
            };
            if !real.is_dir() {
                continue;
            }
            if real.parent() != Some(self.install.as_path()) || !same_name(&real, &name) {
                return Err(SkillError::new(
                    Reason::MixedInstall,
                    format!(
                        "{name} resolves outside this installation: {}",
                        real.display()
                    ),
                )
                .with_source(name));
            }
        }
        Ok(())
    }

    fn boundary(&self, root: &SourceRoot) -> Result<PathBuf, SkillError> {
        let name = match root {
            SourceRoot::Shared => SHARED_DIR,
            SourceRoot::Skill(name) => {
                if name == SHARED_DIR || !is_kebab(name) {
                    return Err(SkillError::new(
                        Reason::UnknownRoot,
                        format!("not a Skill name: {name:?}"),
                    ));
                }
                name
            }
        };
        let path = self.install.join(name);
        if fs::symlink_metadata(&path).is_ok_and(|metadata| is_link(&metadata)) {
            return Err(SkillError::new(
                Reason::MixedInstall,
                format!("{name} is not a physical source boundary"),
            ));
        }
        let real = fs::canonicalize(&path).map_err(|_| {
            SkillError::new(
                Reason::UnknownRoot,
                format!("source boundary is absent: {name}"),
            )
        })?;
        if real.parent() != Some(self.install.as_path()) {
            return Err(SkillError::new(
                Reason::MixedInstall,
                format!("{name} resolves outside this installation"),
            ));
        }
        Ok(real)
    }

    /// Resolve `relative` inside `boundary` and enforce the boundary on the real path.
    fn resolve(&self, boundary: &Path, relative: &str) -> Result<PathBuf, SkillError> {
        let mut path = boundary.to_path_buf();
        if !relative.is_empty() {
            validate_relative(relative)?;
            for part in relative.split('/') {
                path.push(part);
                reject_link(&path, Reason::UnsafeLink)?;
            }
        }
        let real = fs::canonicalize(&path)
            .map_err(|error| SkillError::new(Reason::Unreadable, format!("{relative}: {error}")))?;
        if real.starts_with(boundary) {
            Ok(real)
        } else if real.starts_with(&self.install) {
            Err(SkillError::new(
                Reason::BoundaryViolation,
                format!("{relative} resolves into another source boundary"),
            ))
        } else {
            Err(SkillError::new(
                Reason::BoundaryViolation,
                format!("{relative} resolves outside the installation"),
            ))
        }
    }
}

fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_type().is_symlink() || metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn reject_link(path: &Path, reason: Reason) -> Result<fs::Metadata, SkillError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| SkillError::new(Reason::Unreadable, error.to_string()))?;
    if is_link(&metadata) {
        return Err(SkillError::new(
            reason,
            format!("{} is not a physical source", path.display()),
        ));
    }
    Ok(metadata)
}

fn same_name(path: &Path, name: &str) -> bool {
    path.file_name().is_some_and(|found| found == name)
}

impl SourceStore for FsStore {
    fn skills(&self) -> Result<Vec<String>, SkillError> {
        self.verify_boundaries()?;
        let mut skills = Vec::new();
        let entries = fs::read_dir(&self.install)
            .map_err(|error| SkillError::new(Reason::Unreadable, error.to_string()))?;
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if name != SHARED_DIR
                && is_kebab(&name)
                && entry.file_type().is_ok_and(|kind| kind.is_dir())
            {
                skills.push(name);
            }
        }
        skills.sort();
        Ok(skills)
    }

    fn read(&self, source: &SourceRef) -> Result<SourceContent, SkillError> {
        let boundary = self.boundary(&source.root)?;
        validate_relative(&source.path)?;
        let real = self.resolve(&boundary, &source.path)?;
        let metadata = fs::metadata(&real)
            .map_err(|error| SkillError::new(Reason::Unreadable, error.to_string()))?;
        if !metadata.is_file() {
            return Err(SkillError::new(
                Reason::NotRegularFile,
                format!("{} is not a regular file", source.path),
            ));
        }
        let bytes = fs::read(&real)
            .map_err(|error| SkillError::new(Reason::Unreadable, error.to_string()))?;
        let text = decode(&bytes)?.to_owned();
        Ok(SourceContent {
            id: SourceId::new(real.to_string_lossy()),
            text,
        })
    }

    fn entries(&self, root: &SourceRoot, dir: &str) -> Result<Vec<DirEntry>, SkillError> {
        let boundary = self.boundary(root)?;
        let real = match self.resolve(&boundary, dir) {
            Ok(real) => real,
            Err(error) if error.reason == Reason::Unreadable => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        if !real.is_dir() {
            return Err(SkillError::new(
                Reason::NotRegularFile,
                format!("{dir} is not a directory"),
            ));
        }
        let mut found = Vec::new();
        let entries = fs::read_dir(&real)
            .map_err(|error| SkillError::new(Reason::Unreadable, error.to_string()))?;
        for entry in entries {
            let entry =
                entry.map_err(|error| SkillError::new(Reason::Unreadable, error.to_string()))?;
            let name = entry.file_name().into_string().map_err(|name| {
                SkillError::new(
                    Reason::InvalidPath,
                    format!("file name is not UTF-8: {name:?}"),
                )
            })?;
            let metadata = reject_link(&entry.path(), Reason::UnsafeLink)?;
            let is_dir = metadata.is_dir();
            found.push(DirEntry { name, is_dir });
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }
}
