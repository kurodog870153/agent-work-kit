//! Public response shapes of the Skill CLI.

use serde::{Deserialize, Serialize};

/// Public CLI envelope shared by all Skill success and failure responses.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Envelope<T> {
    Ok { data: T },
    Error { error: ErrorData },
}

/// Failure of one source in a response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorData {
    pub reason: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChildData {
    pub node: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiscoverData {
    pub schema: &'static str,
    pub skill: String,
    pub node: String,
    pub children: Vec<ChildData>,
    pub failures: Vec<ErrorData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceData {
    pub source: String,
    pub id: String,
    pub kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolveData {
    pub schema: &'static str,
    pub sources: Vec<SourceData>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SwitchData {
    pub schema: &'static str,
    pub reused: Vec<String>,
    pub added: Vec<String>,
    pub dropped: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidateData {
    pub schema: &'static str,
    pub valid: bool,
    pub checked: Vec<String>,
    pub failures: Vec<ErrorData>,
}
