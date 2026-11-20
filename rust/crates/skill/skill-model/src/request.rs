//! Caller-constructible request shapes of the Skill CLI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceDto {
    pub node: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectionDto {
    pub skill: String,
    #[serde(default)]
    pub nodes: Vec<String>,
    #[serde(default)]
    pub references: Vec<ReferenceDto>,
}

/// Caller-constructible candidate set of a resolve or switch request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestDto {
    pub schema: String,
    pub selections: Vec<SelectionDto>,
    #[serde(default)]
    pub modules: Vec<String>,
}
