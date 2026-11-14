//! Shared structural models for the four public query payloads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractIndex {
    pub contract_id: String,
    pub kind: String,
    pub caller_constructible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ListData {
    pub commands: Vec<String>,
    pub contracts: Vec<ContractIndex>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractDescriptionData {
    pub contract_id: String,
    pub kind: String,
    pub fields: BTreeMap<String, Value>,
    pub scenarios: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldDescriptionData {
    pub contract_id: String,
    pub field: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDescriptionData {
    pub command: String,
    pub arguments: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_contract: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_contract: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaffoldData {
    pub contract_id: String,
    pub scaffold: BTreeMap<String, Value>,
    pub unresolved_paths: Vec<String>,
    pub selected_branches: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExampleData {
    pub contract_id: String,
    pub scenario: String,
    pub valid: bool,
    pub example: Value,
}

pub fn validate(kind: &str, data: Value) -> Result<(), serde_json::Error> {
    match kind {
        "list" => {
            serde_json::from_value::<ListData>(data)?;
        }
        "describe" => {
            if data.get("command").is_some() {
                serde_json::from_value::<CommandDescriptionData>(data)?;
            } else if data.get("field").is_some() {
                serde_json::from_value::<FieldDescriptionData>(data)?;
            } else {
                serde_json::from_value::<ContractDescriptionData>(data)?;
            }
        }
        "scaffold" => {
            serde_json::from_value::<ScaffoldData>(data)?;
        }
        "example" => {
            serde_json::from_value::<ExampleData>(data)?;
        }
        _ => panic!("unknown query kind: {kind}"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn query_data_shapes_reject_missing_and_extra_fields() {
        assert!(validate("list", json!({"commands":[],"contracts":[]})).is_ok());
        assert!(validate("list", json!({"commands":[]})).is_err());
        assert!(
            validate(
                "example",
                json!({"contract_id":"x","scenario":"s","valid":true,"example":{}})
            )
            .is_ok()
        );
        assert!(
            validate(
                "example",
                json!({"contract_id":"x","scenario":"s","valid":true,"example":{},"request":{}})
            )
            .is_err()
        );
    }
}
