//! Ports implemented by the Infrastructure layer.

use skill_model::{SkillError, SourceId, SourceRef, SourceRoot};

/// A validated, decoded source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContent {
    /// Normalized physical source identity; repeated selections of the same source share it.
    pub id: SourceId,
    pub text: String,
}

/// One direct child of a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
}

/// Read-only access to one installation's Shared and Skill sources.
///
/// Implementations enforce the source boundary: regular files only, strict
/// UTF-8 with at most one BOM, and no link may leave its own boundary.
pub trait SourceStore {
    /// Names of the Skills installed beside Shared.
    fn skills(&self) -> Result<Vec<String>, SkillError>;

    /// Read and validate one source.
    fn read(&self, source: &SourceRef) -> Result<SourceContent, SkillError>;

    /// List the direct children of a directory; a missing directory is empty.
    fn entries(&self, root: &SourceRoot, dir: &str) -> Result<Vec<DirEntry>, SkillError>;
}
