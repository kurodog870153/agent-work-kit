//! Public contract query payloads.
//!
//! Every payload is an explicitly typed value; there is no dynamic JSON
//! container anywhere in this crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::model::{ContractKind, ContractValue, FieldType};

/// One contract entry of a list payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractIndex {
    pub contract_id: String,
    pub kind: ContractKind,
    pub caller_constructible: bool,
}

/// Payload of a contract list query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListData {
    pub commands: Vec<String>,
    pub contracts: Vec<ContractIndex>,
}

/// Summary of one field in a contract description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldSummary {
    pub name: String,
    pub field_type: FieldType,
    #[serde(default)]
    pub nullable: bool,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default)]
    pub details_available: bool,
}

/// Payload describing a whole contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractDescriptionData {
    pub contract_id: String,
    pub kind: ContractKind,
    pub fields: BTreeMap<String, FieldSummary>,
    pub scenarios: Vec<String>,
}

/// Detail of one selected field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDescriptionData {
    pub contract_id: String,
    pub path: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field_type: Option<FieldType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nullable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constant: Option<ContractValue>,
}

/// Summary of one command argument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArgumentDescription {
    pub name: String,
    pub flags: Vec<String>,
    pub required: bool,
    pub value_type: ArgumentValueType,
    pub choices: Vec<String>,
    pub exclusive_with: Vec<String>,
    pub help: String,
}

/// Value type accepted by a command argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentValueType {
    String,
    Boolean,
    Array,
}

/// Payload describing one command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDescriptionData {
    pub command: String,
    pub arguments: Vec<ArgumentDescription>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_contract: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_contract: Option<String>,
}

/// Payload of a scaffold query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaffoldData {
    pub contract_id: String,
    pub scaffold: ContractValue,
    pub unresolved_paths: Vec<String>,
    pub selected_branches: BTreeMap<String, String>,
}

/// Payload of an example query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExampleData {
    pub contract_id: String,
    pub scenario: String,
    pub valid: bool,
    pub example: ContractValue,
}
