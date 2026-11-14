//! Structural checks shared by the build script and negative tests.

use std::collections::BTreeSet;
use std::path::Path;

use serde_json::Value;

pub fn read_json_file(path: &Path) -> Result<Value, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("{}: invalid JSON: {error}", path.display()))
}

pub fn validate_example_catalog(entries: &Value) -> Result<(), String> {
    let mut keys = BTreeSet::new();
    for entry in array(entries, "example catalog")? {
        let id = entry["contract_id"]
            .as_str()
            .ok_or("missing example contract ID")?;
        let scenario = entry["scenario"].as_str().ok_or("missing scenario")?;
        let file = entry["file"].as_str().ok_or("missing example filename")?;
        if scenario.is_empty()
            || !file.ends_with(".json")
            || file.contains('/')
            || file.contains('\\')
        {
            return Err(format!("invalid example source: {id}/{scenario}"));
        }
        if !keys.insert((id, scenario)) {
            return Err(format!("duplicate scenario: {id}/{scenario}"));
        }
    }
    Ok(())
}

fn object<'a>(value: &'a Value, name: &str) -> Result<&'a serde_json::Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{name} must be an object"))
}

fn array<'a>(value: &'a Value, name: &str) -> Result<&'a Vec<Value>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{name} must be an array"))
}

pub fn validate_registry(registry: &Value) -> Result<(), String> {
    if registry["schema"] != "public-contract-registry" {
        return Err("invalid registry schema".into());
    }
    let commands = object(&registry["commands"], "commands")?;
    let contracts = object(&registry["contracts"], "contracts")?;
    let examples = object(&registry["examples"], "examples")?;
    let query_schemas = object(&registry["query_data_schemas"], "query data schemas")?;

    for id in [
        "contract-list-data",
        "contract-describe-data",
        "contract-scaffold-data",
        "contract-example-data",
    ] {
        object(
            query_schemas
                .get(id)
                .ok_or_else(|| format!("missing query data schema: {id}"))?,
            id,
        )?;
    }
    for (id, record) in contracts {
        let description = object(&record["description"], id)?;
        if description.get("id").and_then(Value::as_str) != Some(id) {
            return Err(format!("contract ID mismatch: {id}"));
        }
        if !matches!(
            description.get("kind").and_then(Value::as_str),
            Some("semantic_request" | "generated_request" | "response" | "artifact" | "envelope")
        ) {
            return Err(format!("invalid contract kind: {id}"));
        }
        if !description
            .get("caller_constructible")
            .is_some_and(Value::is_boolean)
        {
            return Err(format!("missing caller_constructible: {id}"));
        }
        let mut names = BTreeSet::new();
        let required: BTreeSet<_> = array(&description["required"], "required fields")?
            .iter()
            .map(|name| {
                name.as_str()
                    .ok_or_else(|| format!("invalid required field: {id}"))
            })
            .collect::<Result<_, _>>()?;
        let optional: BTreeSet<_> = array(&description["optional"], "optional fields")?
            .iter()
            .map(|name| {
                name.as_str()
                    .ok_or_else(|| format!("invalid optional field: {id}"))
            })
            .collect::<Result<_, _>>()?;
        if !required.is_disjoint(&optional) {
            return Err(format!("required and optional fields overlap: {id}"));
        }
        for field in array(&description["fields"], "contract fields")? {
            let name = field["name"]
                .as_str()
                .ok_or_else(|| format!("missing field name: {id}"))?;
            if !names.insert(name) {
                return Err(format!("duplicate field: {id}/{name}"));
            }
            if field["required"] != required.contains(name)
                || required.contains(name) == optional.contains(name)
            {
                return Err(format!("field presence metadata mismatch: {id}/{name}"));
            }
            if let Some(reference) = field["reference"].as_str()
                && !contracts.contains_key(reference)
            {
                return Err(format!("missing contract reference: {id}/{reference}"));
            }
        }
        if names != required.union(&optional).copied().collect() {
            return Err(format!("field presence list mismatch: {id}"));
        }
        let order: Vec<_> = array(&description["canonical_order"], "canonical order")?
            .iter()
            .map(|name| {
                name.as_str()
                    .ok_or_else(|| format!("invalid canonical field: {id}"))
            })
            .collect::<Result<_, _>>()?;
        if order.len() != names.len() || order.iter().copied().collect::<BTreeSet<_>>() != names {
            return Err(format!("canonical order mismatch: {id}"));
        }
    }
    for (path, command) in commands {
        let name = command["name"]
            .as_str()
            .ok_or_else(|| format!("missing command name: {path}"))?;
        if path.split(' ').next_back() != Some(name) {
            return Err(format!("command path mismatch: {path}"));
        }
        let mut names = BTreeSet::new();
        let mut flags = BTreeSet::new();
        for argument in array(&command["arguments"], "command arguments")? {
            let name = argument["name"]
                .as_str()
                .ok_or_else(|| format!("missing argument name: {path}"))?;
            if !names.insert(name) {
                return Err(format!("duplicate argument: {path}/{name}"));
            }
            for flag in array(&argument["flags"], "argument flags")? {
                let flag = flag
                    .as_str()
                    .ok_or_else(|| format!("invalid flag: {path}/{name}"))?;
                if !flags.insert(flag) {
                    return Err(format!("duplicate flag: {path}/{flag}"));
                }
            }
        }
        for group in array(&command["exclusive_groups"], "exclusive groups")? {
            for member in array(&group["members"], "group members")? {
                let member = member
                    .as_str()
                    .ok_or_else(|| format!("invalid group member: {path}"))?;
                if !names.contains(member) {
                    return Err(format!("unknown group member: {path}/{member}"));
                }
            }
        }
        for link in ["request_contract", "response_contract"] {
            if let Some(id) = command[link].as_str()
                && !contracts.contains_key(id)
            {
                return Err(format!("missing command contract: {path}/{id}"));
            }
        }
    }
    for (id, scenarios) in examples {
        if !contracts.contains_key(id) {
            return Err(format!("example has unknown contract: {id}"));
        }
        for (scenario, value) in object(scenarios, "scenarios")? {
            if scenario.is_empty() {
                return Err(format!("empty scenario: {id}"));
            }
            if !value["valid"].is_boolean() || value.get("example").is_none() {
                return Err(format!("invalid example: {id}/{scenario}"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_validation_rejects_missing_links_and_malformed_resources() {
        let original = crate::registry().clone();
        assert!(validate_registry(&original).is_ok());
        let mut value = original.clone();
        value["commands"]["tooling policy load"]["request_contract"] = "missing".into();
        assert!(
            validate_registry(&value)
                .unwrap_err()
                .contains("missing command contract")
        );
        let mut value = original.clone();
        value["contracts"]["tooling-cli-result"]["description"]["fields"][0]["reference"] =
            "missing".into();
        assert!(
            validate_registry(&value)
                .unwrap_err()
                .contains("missing contract reference")
        );
        let mut value = original.clone();
        value["examples"]["tooling-cli-result"]["success"]["valid"] = Value::Null;
        assert!(
            validate_registry(&value)
                .unwrap_err()
                .contains("invalid example")
        );
        let mut value = original.clone();
        value["contracts"]["tooling-policy-load-request"]["description"]["fields"][2]["required"] =
            Value::Bool(false);
        assert!(
            validate_registry(&value)
                .unwrap_err()
                .contains("field presence metadata mismatch")
        );
        let mut value = original.clone();
        value["contracts"]["tooling-policy-load-request"]["description"]["optional"] =
            serde_json::json!(["previous", "semantic_review"]);
        assert!(
            validate_registry(&value)
                .unwrap_err()
                .contains("required and optional fields overlap")
        );
        let mut value = original;
        value["query_data_schemas"]
            .as_object_mut()
            .unwrap()
            .remove("contract-example-data");
        assert!(
            validate_registry(&value)
                .unwrap_err()
                .contains("missing query data schema")
        );
    }

    #[test]
    fn source_reader_rejects_missing_and_invalid_json_files() {
        let missing =
            std::env::temp_dir().join(format!("contract-registry-missing-{}", std::process::id()));
        assert!(read_json_file(&missing).is_err());
        let invalid = std::env::temp_dir().join(format!(
            "contract-registry-invalid-{}.json",
            std::process::id()
        ));
        std::fs::write(&invalid, b"{").unwrap();
        assert!(
            read_json_file(&invalid)
                .unwrap_err()
                .contains("invalid JSON")
        );
        std::fs::remove_file(invalid).unwrap();
    }

    #[test]
    fn duplicate_scenarios_and_escaped_example_files_are_rejected() {
        let entry = serde_json::json!({"contract_id":"x","scenario":"s","file":"a.json"});
        assert!(
            validate_example_catalog(&serde_json::json!([entry.clone(), entry]))
                .unwrap_err()
                .contains("duplicate scenario")
        );
        assert!(
            validate_example_catalog(
                &serde_json::json!([{"contract_id":"x","scenario":"s","file":"../a.json"}])
            )
            .is_err()
        );
    }
}
