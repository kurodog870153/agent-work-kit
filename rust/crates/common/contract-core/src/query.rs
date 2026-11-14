//! On-demand public contract queries. Business validation remains in each domain.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

use crate::{command, contract, example, registry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryError {
    pub reason: &'static str,
    pub message: String,
}

impl QueryError {
    fn new(reason: &'static str, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }
}

pub type QueryResult = Result<Value, QueryError>;

fn payload(kind: &str, value: Value) -> QueryResult {
    crate::payload::validate(kind, value.clone())
        .map_err(|error| QueryError::new("invalid_registry_payload", error.to_string()))?;
    Ok(value)
}

fn record(id: &str) -> Result<&'static Value, QueryError> {
    contract(id)
        .ok_or_else(|| QueryError::new("unknown_contract_id", format!("Unknown contract ID: {id}")))
}

pub fn list(scope: Option<&str>, prefix: Option<&str>) -> QueryResult {
    if !matches!(scope, None | Some("commands" | "contracts")) {
        return Err(QueryError::new(
            "invalid_scope",
            "Scope must be commands or contracts.",
        ));
    }
    let prefix = prefix.unwrap_or("");
    let commands: Vec<Value> = if scope != Some("contracts") {
        registry()["commands"]
            .as_object()
            .expect("command index")
            .keys()
            .filter(|path| path.starts_with(prefix))
            .map(|path| json!(path))
            .collect()
    } else {
        Vec::new()
    };
    let contracts: Vec<Value> = if scope != Some("commands") {
        registry()["contracts"]
            .as_object()
            .expect("contract index")
            .iter()
            .filter(|(id, _)| id.starts_with(prefix))
            .map(|(id, record)| {
                json!({"contract_id":id,"kind":record["description"]["kind"],
                "caller_constructible":record["description"]["caller_constructible"]})
            })
            .collect()
    } else {
        Vec::new()
    };
    payload("list", json!({"commands":commands,"contracts":contracts}))
}

pub fn describe_command(path: &str) -> QueryResult {
    let node = command(path)
        .ok_or_else(|| QueryError::new("unknown_command", format!("Unknown command: {path}")))?;
    let arguments: Vec<Value> = node["arguments"].as_array().expect("command arguments").iter().map(|argument| {
        let name = argument["name"].as_str().expect("argument name");
        let exclusive_with: Vec<&str> = node["exclusive_groups"].as_array().expect("groups").iter()
            .filter_map(|group| group["members"].as_array())
            .filter(|members| members.iter().any(|member| member == name))
            .flat_map(|members| members.iter().filter_map(Value::as_str).filter(|member| *member != name))
            .collect();
        json!({"name":name,"flags":argument["flags"],"required":argument["required"],
            "type":if argument["boolean"] == true { "boolean" } else if argument["append"] == true || argument["nargs"] == "*" { "array" } else { "string" },
            "choices":argument["choices"],"exclusive_with":exclusive_with,"help":argument["help"]})
    }).collect();
    let mut result = json!({"command":path,"arguments":arguments});
    for link in ["request_contract", "response_contract"] {
        if let Some(id) = node[link].as_str() {
            result[link] = json!(id);
        }
    }
    payload("describe", result)
}

fn decode_pointer(pointer: &str) -> Result<Vec<String>, QueryError> {
    if !pointer.starts_with('/') || pointer.len() == 1 {
        return Err(QueryError::new(
            "invalid_pointer",
            "A field pointer must start with / and name a field.",
        ));
    }
    pointer[1..]
        .split('/')
        .map(|segment| {
            let mut decoded = String::new();
            let mut chars = segment.chars();
            while let Some(ch) = chars.next() {
                if ch == '~' {
                    match chars.next() {
                        Some('0') => decoded.push('~'),
                        Some('1') => decoded.push('/'),
                        _ => {
                            return Err(QueryError::new(
                                "invalid_pointer",
                                "Invalid RFC 6901 escape.",
                            ));
                        }
                    }
                } else {
                    decoded.push(ch);
                }
            }
            Ok(decoded)
        })
        .collect()
}

fn encode_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn nullable(kind: &str) -> bool {
    kind.starts_with("nullable_") || kind.split('|').any(|part| part.trim() == "null")
}

fn resolve_schema<'a>(schema: &'a Value, root: &'a Value) -> Result<&'a Value, QueryError> {
    let mut current = schema;
    for _ in 0..32 {
        let Some(reference) = current["$ref"].as_str() else {
            return Ok(current);
        };
        let pointer = reference.strip_prefix('#').ok_or_else(|| {
            QueryError::new(
                "invalid_reference",
                "External schema references are unavailable.",
            )
        })?;
        current = root.pointer(pointer).ok_or_else(|| {
            QueryError::new(
                "invalid_reference",
                format!("Unknown schema reference: {reference}"),
            )
        })?;
    }
    Err(QueryError::new(
        "invalid_reference",
        "Schema reference cycle.",
    ))
}

fn discriminator(schema: &Value) -> Option<&Value> {
    schema
        .get("discriminator")
        .filter(|value| value.is_object())
}

fn path_candidates<'a>(
    schema: &'a Value,
    root: &'a Value,
    tokens: &[String],
    depth: usize,
) -> Result<Vec<&'a Value>, QueryError> {
    if depth > 32 {
        return Err(QueryError::new(
            "invalid_reference",
            "Schema nesting is too deep.",
        ));
    }
    let schema = resolve_schema(schema, root)?;
    if tokens.is_empty() {
        return Ok(vec![schema]);
    }
    if let Some(property) = schema["properties"].get(&tokens[0]) {
        return path_candidates(property, root, &tokens[1..], depth + 1);
    }
    let mut candidates = Vec::new();
    if let Some(variants) = schema["oneOf"]
        .as_array()
        .or_else(|| schema["anyOf"].as_array())
    {
        for variant in variants {
            candidates.extend(path_candidates(variant, root, tokens, depth + 1)?);
        }
    }
    Ok(candidates)
}

fn field_details(id: &str, pointer: &str, variant: Option<&str>) -> QueryResult {
    let tokens = decode_pointer(pointer)?;
    let description = &record(id)?["description"];
    let field = description["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .find(|field| field["name"] == tokens[0])
        .ok_or_else(|| QueryError::new("unknown_field", format!("Unknown field: {pointer}")))?;
    let root = field["constraints"].get("schema");
    let schema = if tokens.len() == 1 {
        root
    } else {
        let root = root
            .ok_or_else(|| QueryError::new("unknown_field", format!("Unknown field: {pointer}")))?;
        let mut distinct = Vec::new();
        for candidate in path_candidates(root, root, &tokens[1..], 0)? {
            if !distinct.contains(&candidate) {
                distinct.push(candidate);
            }
        }
        if distinct.len() > 1 {
            return Err(QueryError::new(
                "ambiguous_field",
                format!("Field appears in incompatible branches: {pointer}"),
            ));
        }
        distinct.into_iter().next()
    };
    let mut details = json!({"path":pointer,"name":tokens.last().expect("field token")});
    if tokens.len() == 1 {
        details["type"] = field["type"].clone();
        details["required"] = field["required"].clone();
        details["nullable"] = json!(field["type"].as_str().is_some_and(nullable));
        if !field["reference"].is_null() {
            details["reference"] = field["reference"].clone();
        }
    }
    if let Some(schema) = schema {
        let schema = resolve_schema(schema, root.expect("schema root"))?;
        if let Some(discriminator) = discriminator(schema) {
            let mapping = discriminator["mapping"].as_object().ok_or_else(|| {
                QueryError::new("invalid_discriminator", "Missing discriminator mapping.")
            })?;
            details["discriminator"] = discriminator["propertyName"].clone();
            details["variants"] = json!(mapping.keys().collect::<Vec<_>>());
            if let Some(variant) = variant {
                let reference = mapping
                    .get(variant)
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        QueryError::new("unknown_variant", format!("Unknown variant: {variant}"))
                    })?;
                let target = json!({"$ref":reference});
                details["variant"] = json!(variant);
                details["constraints"] =
                    resolve_schema(&target, root.expect("schema root"))?.clone();
            }
        } else {
            if variant.is_some() {
                return Err(QueryError::new(
                    "variant_not_applicable",
                    "The selected field has no discriminator.",
                ));
            }
            details["constraints"] = schema.clone();
        }
    } else {
        if tokens.len() > 1 {
            return Err(QueryError::new(
                "unknown_field",
                format!("Unknown field: {pointer}"),
            ));
        }
        if variant.is_some() {
            return Err(QueryError::new(
                "variant_not_applicable",
                "The selected field has no discriminator.",
            ));
        }
        details["constraints"] = field["constraints"].clone();
    }
    payload("describe", json!({"contract_id":id,"field":details}))
}

pub fn describe_contract(id: &str, field: Option<&str>, variant: Option<&str>) -> QueryResult {
    if let Some(field) = field {
        return field_details(id, field, variant);
    }
    if variant.is_some() {
        return Err(QueryError::new(
            "variant_requires_field",
            "--variant requires --field.",
        ));
    }
    let description = &record(id)?["description"];
    let fields: Map<String, Value> = description["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .map(|field| {
            let name = field["name"].as_str().expect("field name").to_owned();
            let mut summary = json!({"type":field["type"]});
            if field["required"] == false {
                summary["required"] = json!(false);
            }
            if field["type"].as_str().is_some_and(nullable) {
                summary["nullable"] = json!(true);
            }
            if let Some(reference) = field["reference"].as_str() {
                summary["reference"] = json!(reference);
            }
            if !field["constraints"].as_object().is_some_and(Map::is_empty) {
                summary["details_available"] = json!(true);
            }
            (name, summary)
        })
        .collect();
    let scenarios: Vec<&str> = registry()["examples"][id]
        .as_object()
        .map(|items| items.keys().map(String::as_str).collect())
        .unwrap_or_default();
    payload(
        "describe",
        json!({"contract_id":id,"kind":description["kind"],"fields":fields,"scenarios":scenarios}),
    )
}

pub fn example_query(id: &str, scenario: &str) -> QueryResult {
    record(id)?;
    let value = example(id, scenario).ok_or_else(|| {
        QueryError::new("unknown_scenario", format!("Unknown scenario: {scenario}"))
    })?;
    payload(
        "example",
        json!({"contract_id":id,"scenario":scenario,"valid":value["valid"],"example":value["example"]}),
    )
}

fn skeleton_schema(
    schema: &Value,
    root: &Value,
    pointer: &str,
    branches: &BTreeMap<String, String>,
    used: &mut BTreeSet<String>,
    unresolved: &mut BTreeSet<String>,
    depth: usize,
) -> Result<Value, QueryError> {
    if depth > 32 {
        return Err(QueryError::new(
            "invalid_reference",
            "Schema nesting is too deep.",
        ));
    }
    let schema = resolve_schema(schema, root)?;
    if let Some(value) = schema.get("const") {
        return Ok(value.clone());
    }
    if let Some(discriminator) = discriminator(schema) {
        let Some(variant) = branches.get(pointer) else {
            unresolved.insert(pointer.to_owned());
            return Ok(Value::Null);
        };
        let reference = discriminator["mapping"][variant].as_str().ok_or_else(|| {
            QueryError::new(
                "unknown_variant",
                format!("Unknown variant at {pointer}: {variant}"),
            )
        })?;
        used.insert(pointer.to_owned());
        return skeleton_schema(
            &json!({"$ref":reference}),
            root,
            pointer,
            branches,
            used,
            unresolved,
            depth + 1,
        );
    }
    if let Some(required) = schema["required"].as_array() {
        let mut object = Map::new();
        for name in required {
            let name = name.as_str().expect("required field name");
            let child = schema["properties"].get(name).ok_or_else(|| {
                QueryError::new(
                    "invalid_schema",
                    format!("Missing required property: {name}"),
                )
            })?;
            let path = format!("{pointer}/{}", encode_segment(name));
            object.insert(
                name.to_owned(),
                skeleton_schema(child, root, &path, branches, used, unresolved, depth + 1)?,
            );
        }
        return Ok(Value::Object(object));
    }
    if schema["type"] == "object" {
        return Ok(json!({}));
    }
    if schema["type"] == "array" && schema["minItems"].as_u64().unwrap_or(0) == 0 {
        return Ok(json!([]));
    }
    unresolved.insert(pointer.to_owned());
    Ok(Value::Null)
}

pub fn scaffold(id: &str, branch_args: &[String]) -> QueryResult {
    let description = &record(id)?["description"];
    if description["kind"] == "generated_request" {
        return Err(QueryError::new(
            "generated_request_not_caller_constructible",
            "Generated requests must come from their formal prepare flow.",
        ));
    }
    if description["kind"] != "semantic_request" || description["caller_constructible"] != true {
        return Err(QueryError::new(
            "contract_scaffold_requires_request",
            "Only caller-constructible requests can be scaffolded.",
        ));
    }
    let mut branches = BTreeMap::new();
    for raw in branch_args {
        let (pointer, variant) = raw.split_once('=').ok_or_else(|| {
            QueryError::new("invalid_branch", "Branch must be <pointer>=<variant>.")
        })?;
        decode_pointer(pointer)?;
        if variant.is_empty() {
            return Err(QueryError::new(
                "invalid_branch",
                "Branch variant is empty.",
            ));
        }
        if branches
            .insert(pointer.to_owned(), variant.to_owned())
            .is_some()
        {
            return Err(QueryError::new(
                "duplicate_branch",
                format!("Duplicate branch: {pointer}"),
            ));
        }
    }
    let mut object = Map::new();
    let mut unresolved = BTreeSet::new();
    let mut used = BTreeSet::new();
    for field in description["fields"].as_array().expect("fields") {
        if field["required"] != true {
            continue;
        }
        let name = field["name"].as_str().expect("field name");
        let pointer = format!("/{}", encode_segment(name));
        let value = if name == "schema" {
            json!(id)
        } else if let Some(value) = field["constraints"].get("const") {
            value.clone()
        } else if let Some(schema) = field["constraints"].get("schema") {
            skeleton_schema(
                schema,
                schema,
                &pointer,
                &branches,
                &mut used,
                &mut unresolved,
                0,
            )?
        } else if field["type"] == "array" {
            json!([])
        } else {
            unresolved.insert(pointer.clone());
            Value::Null
        };
        object.insert(name.to_owned(), value);
    }
    for pointer in branches.keys() {
        if !used.contains(pointer) {
            return Err(QueryError::new(
                "invalid_branch",
                format!("Branch is unknown or its parent was not selected: {pointer}"),
            ));
        }
    }
    payload(
        "scaffold",
        json!({"contract_id":id,"scaffold":object,"unresolved_paths":unresolved,"selected_branches":branches}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_and_default_description_are_small_and_sorted() {
        let value = list(None, None).unwrap();
        assert!(value["commands"].as_array().unwrap().len() >= 120);
        assert!(value["contracts"].as_array().unwrap().len() >= 98);
        assert!(value.to_string().len() < 25000);
        let description = describe_contract("work-discussion-request", None, None).unwrap();
        assert!(description.to_string().len() < 3000);
        assert!(description.get("example").is_none());
        let nullable_field = describe_contract("work-command-result", None, None).unwrap();
        assert_eq!(nullable_field["fields"]["exit_code"]["nullable"], true);
    }

    #[test]
    fn branch_selection_is_order_independent_and_rejects_unknown() {
        let id = "work-discussion-request";
        let first = scaffold(id, &["/command=read".into()]).unwrap();
        assert_eq!(first["scaffold"]["command"]["kind"], "read");
        assert!(scaffold(id, &["/command=unknown".into()]).is_err());
        assert!(scaffold(id, &["/command=read".into(), "/command=read".into()]).is_err());
        let parent = "/command=update".to_owned();
        let child = "/command/operation/change=update_context".to_owned();
        assert_eq!(
            scaffold(id, &[parent.clone(), child.clone()]).unwrap(),
            scaffold(id, &[child.clone(), parent]).unwrap()
        );
        assert!(scaffold(id, &[child]).is_err());
    }

    #[test]
    fn pointer_and_example_queries_reject_invalid_selections() {
        assert!(
            describe_contract("work-discussion-request", Some("/command"), None).unwrap()["field"]
                ["variants"]
                .is_array()
        );
        assert!(describe_contract("work-discussion-request", Some("/command"), Some("read")).unwrap()["field"]["constraints"].is_object());
        assert!(
            describe_contract("work-discussion-request", Some("/command"), Some("missing"))
                .is_err()
        );
        assert!(
            describe_contract(
                "work-discussion-request",
                Some("/command/operation/change"),
                None
            )
            .unwrap()["field"]["variants"]
                .is_array()
        );
        assert!(describe_contract("work-discussion-request", Some("/bad~2name"), None).is_err());
        assert_eq!(
            example_query("tooling-policy-load-request", "invalid-missing-previous").unwrap()["valid"],
            false
        );
        assert!(example_query("tooling-policy-load-request", "missing").is_err());
    }

    #[test]
    fn nested_branches_use_data_pointers_and_require_parent_selection() {
        let schema = json!({
            "type":"object","required":["child/one"],
            "properties":{"child/one":{"discriminator":{"propertyName":"kind","mapping":{"a":"#/$defs/A"}},"oneOf":[{"$ref":"#/$defs/A"}]}},
            "$defs":{"A":{"type":"object","required":["kind","nested"],"properties":{"kind":{"const":"a"},"nested":{"discriminator":{"propertyName":"mode","mapping":{"x":"#/$defs/X"}},"oneOf":[{"$ref":"#/$defs/X"}]}}},
                "X":{"type":"object","required":["mode","value"],"properties":{"mode":{"const":"x"},"value":{"type":"string"}}}}
        });
        let branches = BTreeMap::from([
            ("/root/child~1one".into(), "a".into()),
            ("/root/child~1one/nested".into(), "x".into()),
        ]);
        let mut used = BTreeSet::new();
        let mut unresolved = BTreeSet::new();
        let value = skeleton_schema(
            &schema,
            &schema,
            "/root",
            &branches,
            &mut used,
            &mut unresolved,
            0,
        )
        .unwrap();
        assert_eq!(value["child/one"]["nested"]["mode"], "x");
        assert!(unresolved.contains("/root/child~1one/nested/value"));
        assert_eq!(used.len(), 2);
    }
}
