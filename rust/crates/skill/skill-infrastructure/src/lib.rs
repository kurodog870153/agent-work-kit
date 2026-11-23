//! Skill domain technical facts: a file-system `SourceStore`.
//!
//! Every read resolves the real path first. A source is accepted only when its
//! real path stays inside the boundary that owns it (Shared or one Skill), so
//! symlinks and junctions may alias inside a boundary but never cross one.
//! Unmanaged Host Skills are outside this installation's source inventory.

use std::fs;
use std::path::{Path, PathBuf};

use skill_feature::{DirEntry, SourceContent, SourceStore};
use skill_model::{Reason, SkillError, SourceId, SourceRef, SourceRoot};
use skill_operations::decode;
use skill_operations::path::validate_relative;

/// Directory name of the single Shared boundary inside an installation.
pub const SHARED_DIR: &str = "shared";

/// Skill roots owned by this installation, not every Skill exposed by the Host.
const MANAGED_SKILLS: [&str; 2] = ["tooling", "work"];

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
            if name != SHARED_DIR && !is_managed_skill(&name) {
                continue;
            }
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
                if !is_managed_skill(name) {
                    return Err(SkillError::new(
                        Reason::UnknownRoot,
                        format!("not a Skill name: {name:?}"),
                    ));
                }
                name
            }
        };
        let path = self.install.join(name);
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
            path.extend(relative.split('/'));
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
                Reason::LinkEscape,
                format!("{relative} resolves outside the installation"),
            ))
        }
    }
}

fn is_managed_skill(name: &str) -> bool {
    MANAGED_SKILLS.contains(&name)
}

fn same_name(path: &Path, name: &str) -> bool {
    path.file_name().is_some_and(|found| found == name)
}

impl SourceStore for FsStore {
    fn skills(&self) -> Result<Vec<String>, SkillError> {
        let mut skills = Vec::new();
        for name in MANAGED_SKILLS {
            let path = self.install.join(name);
            if fs::canonicalize(path).is_ok_and(|real| {
                real.is_dir()
                    && real.parent() == Some(self.install.as_path())
                    && same_name(&real, name)
            }) {
                skills.push(name.to_owned());
            }
        }
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
            // Follow links here only to classify; the real path is checked on read.
            let is_dir = fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_dir());
            found.push(DirEntry { name, is_dir });
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }
}
