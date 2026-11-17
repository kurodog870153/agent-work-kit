//! Skill domain model: document shapes, source identity and failure vocabulary.
//!
//! Nothing here touches files or parses text; the model only fixes the closed
//! set of shapes the other layers exchange.

use std::fmt;

use serde::{Deserialize, Serialize};

mod description;
mod example;
mod request;
mod response;

pub use description::{
    DISCOVER_RESPONSE, DOMAIN, RESOLVE_REQUEST, RESOLVE_RESPONSE, SWITCH_RESPONSE,
    VALIDATE_RESPONSE, contract_records,
};
pub use example::contract_examples;
pub use request::{ReferenceDto, RequestDto, SelectionDto};
pub use response::{
    ChildData, DiscoverData, Envelope, ErrorData, ResolveData, SourceData, SwitchData, ValidateData,
};

/// Declared strength of a normative instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum InstructionStrength {
    Required,
    Default,
    Recommended,
}

impl InstructionStrength {
    pub const ALL: [Self; 3] = [Self::Required, Self::Default, Self::Recommended];

    /// Marker text written at the start of a normative instruction.
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Required => "[Required]",
            Self::Default => "[Default]",
            Self::Recommended => "[Recommended]",
        }
    }
}

/// Nested `metadata` block of the closed frontmatter contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetadataBlock {
    pub tags: Vec<String>,
}

/// Closed frontmatter contract of a Root, Node or Reference document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
    pub metadata: MetadataBlock,
}

/// One declared Module or Reference row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    /// Direct Markdown file name such as `cli-entry.md`.
    pub name: String,
    pub condition: String,
}

/// Kind of declared conditional source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteKind {
    Module,
    Reference,
}

impl RouteKind {
    /// Second-level heading that declares this kind.
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Module => "Modules",
            Self::Reference => "References",
        }
    }

    /// First table column that names the declared source.
    pub const fn column(self) -> &'static str {
        match self {
            Self::Module => "Module",
            Self::Reference => "Reference",
        }
    }
}

/// Document role, which decides the frontmatter and route rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentKind {
    /// Root or Node `instructions.md`.
    Instruction,
    /// `references/*.md` of a Root or Node.
    Reference,
    /// `skills/shared/bootstrap.md`.
    Bootstrap,
    /// `skills/shared/bootstrap/*.md`.
    Module,
}

impl DocumentKind {
    pub const fn requires_frontmatter(self) -> bool {
        matches!(self, Self::Instruction | Self::Reference)
    }

    /// The only route kind this document may declare.
    pub const fn declares(self) -> Option<RouteKind> {
        match self {
            Self::Instruction => Some(RouteKind::Reference),
            Self::Bootstrap => Some(RouteKind::Module),
            Self::Reference | Self::Module => None,
        }
    }
}

/// A parsed and structurally valid instruction document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub kind: DocumentKind,
    pub frontmatter: Option<Frontmatter>,
    pub routes: Vec<Route>,
}

/// Independent source boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SourceRoot {
    Shared,
    Skill(String),
}

/// A source named relative to one boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceRef {
    pub root: SourceRoot,
    /// Forward-slash relative path.
    pub path: String,
}

impl SourceRef {
    pub fn shared(path: impl Into<String>) -> Self {
        Self {
            root: SourceRoot::Shared,
            path: path.into(),
        }
    }

    pub fn skill(skill: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            root: SourceRoot::Skill(skill.into()),
            path: path.into(),
        }
    }
}

impl fmt::Display for SourceRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.root {
            SourceRoot::Shared => write!(formatter, "shared:{}", self.path),
            SourceRoot::Skill(name) => write!(formatter, "skill({name}):{}", self.path),
        }
    }
}

/// Identity of a source: its real path, so aliases collapse to one source.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceId(String);

impl SourceId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stable machine-readable failure reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Unreadable,
    NotRegularFile,
    InvalidUtf8,
    MultipleBom,
    MissingFrontmatter,
    InvalidFrontmatter,
    InvalidName,
    InvalidTags,
    InvalidRouteTable,
    InvalidRouteName,
    EmptyCondition,
    DuplicateRoute,
    UnexpectedRoutes,
    MissingMarker,
    MultipleMarkers,
    InvalidPath,
    LinkEscape,
    BoundaryViolation,
    MixedInstall,
    UnknownRoot,
    AncestorMissing,
    UndeclaredSource,
    InvalidRequest,
}

impl Reason {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::NotRegularFile => "not_regular_file",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::MultipleBom => "multiple_bom",
            Self::MissingFrontmatter => "missing_frontmatter",
            Self::InvalidFrontmatter => "invalid_frontmatter",
            Self::InvalidName => "invalid_name",
            Self::InvalidTags => "invalid_tags",
            Self::InvalidRouteTable => "invalid_route_table",
            Self::InvalidRouteName => "invalid_route_name",
            Self::EmptyCondition => "empty_condition",
            Self::DuplicateRoute => "duplicate_route",
            Self::UnexpectedRoutes => "unexpected_routes",
            Self::MissingMarker => "missing_marker",
            Self::MultipleMarkers => "multiple_markers",
            Self::InvalidPath => "invalid_path",
            Self::LinkEscape => "link_escape",
            Self::BoundaryViolation => "boundary_violation",
            Self::MixedInstall => "mixed_install",
            Self::UnknownRoot => "unknown_root",
            Self::AncestorMissing => "ancestor_missing",
            Self::UndeclaredSource => "undeclared_source",
            Self::InvalidRequest => "invalid_request",
        }
    }
}

/// What a failed source invalidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureScope {
    /// Operations of every Skill that depends on the Shared Bootstrap.
    SharedBootstrap,
    /// Every selection that depends on the Root or ancestor.
    RootOrAncestor,
    /// Only operations that depend on the failed branch, Module or Reference.
    Branch,
}

impl FailureScope {
    pub const fn code(self) -> &'static str {
        match self {
            Self::SharedBootstrap => "shared_bootstrap",
            Self::RootOrAncestor => "root_or_ancestor",
            Self::Branch => "branch",
        }
    }
}

/// Failure of one source, with the scope it invalidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillError {
    pub reason: Reason,
    pub message: String,
    pub source: Option<String>,
    pub scope: Option<FailureScope>,
    pub selection: Option<String>,
}

impl SkillError {
    pub fn new(reason: Reason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
            source: None,
            scope: None,
            selection: None,
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source.get_or_insert_with(|| source.into());
        self
    }

    #[must_use]
    pub fn with_scope(mut self, scope: FailureScope) -> Self {
        self.scope = Some(scope);
        self
    }

    #[must_use]
    pub fn with_selection(mut self, selection: impl Into<String>) -> Self {
        self.selection = Some(selection.into());
        self
    }
}

impl fmt::Display for SkillError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.reason.code(), self.message)
    }
}

impl std::error::Error for SkillError {}

#[cfg(test)]
mod tests;
