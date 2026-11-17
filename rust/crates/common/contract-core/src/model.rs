//! Explicit contract data model shared by the Work and Skill domains.
//!
//! Each domain owns its contract resources and parses them into these types;
//! `contract-core` never reads or interprets domain resources itself.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Declared role of a public contract.
///
/// Kinds are ordered by declaration, so a semantic request sorts before a
/// response:
///
/// ```
/// use contract_core::model::ContractKind;
///
/// assert!(ContractKind::SemanticRequest < ContractKind::Response);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractKind {
    SemanticRequest,
    GeneratedRequest,
    Response,
    Artifact,
    Envelope,
}

/// Declared type of a contract field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Integer,
    Number,
    Boolean,
    Array,
    Object,
}

/// Narrow, explicitly typed value used by examples, constants and scaffolds.
///
/// This is a contract-level value model, not a general JSON container: it has a
/// closed set of variants and no arbitrary extension points.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ContractValue {
    #[default]
    Null,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
    Array(Vec<ContractValue>),
    Object(BTreeMap<String, ContractValue>),
}

impl ContractValue {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Boolean(value) => Some(*value),
            _ => None,
        }
    }
}

/// Schema branch selector declared by a contract field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Discriminator {
    pub property: String,
    pub mapping: BTreeMap<String, String>,
}

/// Explicit schema subset required to derive structural scaffolds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case", deny_unknown_fields)]
pub enum SchemaNode {
    /// Reference to a definition owned by the same contract record.
    Reference { reference: String },
    /// Fixed value that always overrides branch selection.
    Constant { value: ContractValue },
    /// Object with declared required properties.
    Object {
        #[serde(default)]
        required: Vec<String>,
        #[serde(default)]
        properties: BTreeMap<String, SchemaNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        discriminator: Option<Discriminator>,
    },
    /// Array; `min_items` of zero stays unresolved because no element is required.
    Array {
        items: Box<SchemaNode>,
        #[serde(default)]
        min_items: u64,
    },
    /// Union of alternative schemas.
    Branch { alternatives: Vec<SchemaNode> },
    /// No structural constraint can be derived.
    Open,
}

/// One declared field of a public contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDescription {
    pub name: String,
    pub field_type: FieldType,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constant: Option<ContractValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaNode>,
}

/// Declared shape of a public contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractDescription {
    pub id: String,
    pub kind: ContractKind,
    pub caller_constructible: bool,
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
    #[serde(default)]
    pub canonical_order: Vec<String>,
    #[serde(default)]
    pub fields: Vec<FieldDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<ContractValue>,
}

/// A contract record together with the definitions its schemas reference.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractRecord {
    pub description: ContractDescription,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub definitions: BTreeMap<String, SchemaNode>,
}

/// Argument behaviour of one public command argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentAction {
    Set,
    SetTrue,
    Append,
}

/// One declared argument of a public command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandArgument {
    pub name: String,
    #[serde(default)]
    pub flags: Vec<String>,
    #[serde(default)]
    pub required: bool,
    pub action: ArgumentAction,
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub nargs_any: bool,
    #[serde(default)]
    pub help: String,
}

/// Mutually exclusive argument group of a public command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExclusiveGroup {
    pub members: Vec<String>,
    #[serde(default)]
    pub required: bool,
}

/// One node of a domain command tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandNode {
    pub name: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub arguments: Vec<CommandArgument>,
    #[serde(default)]
    pub exclusive_groups: Vec<ExclusiveGroup>,
    #[serde(default)]
    pub children: Vec<CommandNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_contract: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_contract: Option<String>,
}

/// One declared example scenario of a contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Example {
    pub valid: bool,
    #[serde(default)]
    pub example: ContractValue,
}
