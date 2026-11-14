//! Stateless Tooling command boundary.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{ArgMatches, Command};
use contract_core::query;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tooling_flow::operation;
use tooling_flow::policy::{self, PolicyLoadError, PolicySummary};

pub use tooling_model::common::CliStatus as Status;

#[derive(Debug, Serialize)]
pub struct CliResult {
    pub schema: &'static str,
    pub status: Status,
    pub reason_code: String,
    pub message: &'static str,
    pub data: Value,
    pub execution: host_environment::ExecutionEnvironment,
}

impl CliResult {
    pub fn to_stdout(&self) -> Result<Vec<u8>, serde_json::Error> {
        let _: tooling_model::contract::CliEnvelope =
            serde_json::from_value(serde_json::to_value(self)?)
                .expect("CLI result matches its model");
        serde_json::to_vec_pretty(self)
    }
}

struct CliError {
    code: i32,
    reason: &'static str,
    message: &'static str,
    data: Value,
}

impl CliError {
    fn new(code: i32, reason: &'static str, message: &'static str, data: Value) -> Self {
        Self {
            code,
            reason,
            message,
            data,
        }
    }
}

fn success(data: Value, reason_code: &'static str) -> CliResult {
    CliResult {
        schema: "tooling-cli-result",
        status: Status::Success,
        reason_code: reason_code.into(),
        message: "The command completed successfully.",
        data,
        execution: host_environment::ExecutionEnvironment::inherited(),
    }
}

fn failure(error: CliError) -> (i32, CliResult) {
    (
        error.code,
        CliResult {
            schema: "tooling-cli-result",
            status: if error.code == 8 || error.code == 10 {
                Status::Failed
            } else {
                Status::Rejected
            },
            reason_code: error.reason.into(),
            message: error.message,
            data: error.data,
            execution: host_environment::ExecutionEnvironment::inherited(),
        },
    )
}

fn command() -> Command {
    contract_core::clap_command("tooling")
        .expect("validated Tooling command tree")
        .version(env!("CARGO_PKG_VERSION"))
}

fn query_error(error: query::QueryError) -> CliError {
    CliError::new(
        4,
        error.reason,
        "The contract query is invalid.",
        json!({"detail":error.message}),
    )
}

fn optional<'a>(matches: &'a ArgMatches, name: &str) -> Option<&'a str> {
    matches.get_one::<String>(name).map(String::as_str)
}

fn option<'a>(matches: &'a ArgMatches, name: &str) -> &'a str {
    matches.get_one::<String>(name).expect("required by clap")
}

fn hierarchy_root(raw: &str) -> Result<PathBuf, CliError> {
    policy::hierarchy_root(raw).map_err(|error| {
        let (code, reason, message) = match error {
            policy::PolicyRootError::SkillRootUnavailable => (
                8,
                "skill_root_unavailable",
                "The Skill root is unavailable.",
            ),
            policy::PolicyRootError::SkillIdentityInvalid => (
                4,
                "skill_identity_invalid",
                "The Tooling Skill identity is invalid.",
            ),
            policy::PolicyRootError::SkillEntrypointEscaped => (
                5,
                "skill_entrypoint_escaped",
                "The Tooling Skill entrypoint escapes its root.",
            ),
            policy::PolicyRootError::HierarchyUnavailable => (
                8,
                "hierarchy_unavailable",
                "The Tooling hierarchy is unavailable.",
            ),
            policy::PolicyRootError::HierarchyEscaped => (
                5,
                "hierarchy_escaped",
                "The Tooling hierarchy escapes its Skill root.",
            ),
        };
        CliError::new(code, reason, message, json!({"path":raw}))
    })
}

fn discover(matches: &ArgMatches) -> Result<Value, CliError> {
    let root = hierarchy_root(option(matches, "skill-root"))?;
    let node = option(matches, "node");
    let path = if node == "." {
        root.clone()
    } else {
        if !policy::valid_node(node) {
            return Err(CliError::new(
                4,
                "invalid_node",
                "The node path is invalid.",
                json!({"node":node}),
            ));
        }
        root.join(node)
    };
    let children = policy::discover(&root, &path).map_err(|error| {
        let (code, reason) = match error {
            policy::DiscoveryError::RootMissing | policy::DiscoveryError::ListingFailed => {
                (8, "discovery_unavailable")
            }
            policy::DiscoveryError::CurrentEscaped => (5, "node_outside_skill"),
            _ => (4, "node_invalid"),
        };
        CliError::new(
            code,
            reason,
            "The requested hierarchy node could not be discovered.",
            json!({"node":node,"error":format!("{error:?}")}),
        )
    })?;
    Ok(json!({
        "node": node,
        "children": children.into_iter().map(|child| match child.metadata {
            Ok(metadata) => json!({"name":child.name,"metadata":metadata}),
            Err(error) => json!({"name":child.name,"error":format!("{error:?}")}),
        }).collect::<Vec<_>>()
    }))
}

#[derive(Debug, Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct SemanticReview {
    reviewed: bool,
    compatible: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyLoadRequest {
    selected_paths: Vec<String>,
    applicable_references: Vec<String>,
    #[serde(deserialize_with = "required_nullable_previous")]
    previous: Option<PolicySummary>,
    #[serde(default)]
    semantic_review: Option<SemanticReview>,
}

fn required_nullable_previous<'de, D>(deserializer: D) -> Result<Option<PolicySummary>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<PolicySummary>::deserialize(deserializer)
}

fn read_request<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T, CliError> {
    let path = Path::new(raw).canonicalize().map_err(|_| {
        CliError::new(
            8,
            "input_file_read_failed",
            "The request file could not be read.",
            json!({"path":raw}),
        )
    })?;
    if !path.is_file() {
        return Err(CliError::new(
            8,
            "input_file_read_failed",
            "The request file is not a regular file.",
            json!({"path":raw}),
        ));
    }
    let bytes = fs::read(&path).map_err(|_| {
        CliError::new(
            8,
            "input_file_read_failed",
            "The request file could not be read.",
            json!({"path":raw}),
        )
    })?;
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        CliError::new(
            3,
            "invalid_utf8",
            "The request is not valid UTF-8.",
            json!({"path":raw}),
        )
    })?;
    serde_json::from_str(text).map_err(|error| {
        CliError::new(
            3,
            "invalid_request_json",
            "The request JSON is invalid.",
            json!({"path":raw,"error":error.to_string()}),
        )
    })
}

fn load(matches: &ArgMatches) -> Result<Value, CliError> {
    let root = hierarchy_root(option(matches, "skill-root"))?;
    let request: PolicyLoadRequest = read_request(option(matches, "input-file"))?;
    policy::validate_selection(&request.selected_paths).map_err(|error| {
        CliError::new(
            4,
            "invalid_selection",
            "The selected paths are invalid.",
            json!({"error":format!("{error:?}")}),
        )
    })?;
    let requested: HashSet<_> = request.applicable_references.iter().cloned().collect();
    if requested.len() != request.applicable_references.len() {
        return Err(CliError::new(
            4,
            "invalid_references",
            "Applicable references contain duplicates.",
            json!({}),
        ));
    }
    let observed = RefCell::new(HashSet::new());
    let selected: Vec<_> = request.selected_paths.iter().map(String::as_str).collect();
    let loaded = policy::load(
        &root,
        &selected,
        |node, name, _condition| {
            let path = if node.is_empty() {
                format!("references/{name}.md")
            } else {
                format!("{node}/references/{name}.md")
            };
            observed.borrow_mut().insert(path.clone());
            requested.contains(&path)
        },
        |_| Ok(()),
        request.previous.as_ref(),
    )
    .map_err(|error| match error {
        PolicyLoadError::InvalidPrevious(error) => CliError::new(
            4,
            "invalid_previous_summary",
            "The previous policy summary is invalid.",
            json!({"error":format!("{error:?}")}),
        ),
        PolicyLoadError::Source(error) => CliError::new(
            5,
            "policy_source_invalid",
            "The selected policy sources could not be verified.",
            json!({"error":format!("{error:?}")}),
        ),
    })?;
    let missing: Vec<_> = requested
        .difference(&observed.into_inner())
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(CliError::new(
            4,
            "invalid_references",
            "A requested reference is not declared by the selected policy.",
            json!({"references":missing}),
        ));
    }
    Ok(json!({"sources": loaded.sources.iter().map(|source| json!({
        "relative_path":source.relative_path,
        "raw_sha256":source.raw_sha256,
        "canonical_sha256":source.canonical_sha256,
        "content":String::from_utf8_lossy(&source.canonical_content),
    })).collect::<Vec<_>>(), "semantic_review": {
        "origin":"host_reported",
        "claim":request.semantic_review
    }, "summary":loaded.summary, "change": {
        "added_or_invalidated":loaded.change.added_or_invalidated.iter().map(|source| source.relative_path.clone()).collect::<Vec<_>>(),
        "removed":loaded.change.removed,
    }}))
}

fn evaluate_operation(matches: &ArgMatches) -> Result<(Value, &'static str), CliError> {
    let raw: Value = read_request(option(matches, "input-file"))?;
    if raw.as_object().is_some_and(|value| {
        [
            "policy_state",
            "semantic_review",
            "observed_tools",
            "need",
            "effects",
            "authorization",
            "outcome",
            "reconciliation",
            "alternative",
        ]
        .iter()
        .any(|key| value.contains_key(*key))
    }) {
        return Err(CliError::new(
            4,
            "operation_contract_retired",
            "Host tool selection, authorization, outcome and fallback claims are no longer evaluated by Tooling CLI.",
            json!({}),
        ));
    }
    let request: operation::EvaluateRequest = serde_json::from_value(raw).map_err(|_| {
        CliError::new(
            3,
            "invalid_request_json",
            "The operation target request is invalid.",
            json!({}),
        )
    })?;
    let assessment = operation::evaluate(request).map_err(|reason| {
        CliError::new(
            4,
            reason,
            "The operation target is unavailable or outside the project root.",
            json!({}),
        )
    })?;
    Ok((
        json!({
            "target_valid":true,
            "canonical_target":assessment.canonical_target,
            "authorization_verified":false,
            "tool_fit_verified":false,
            "outcome_verified":false
        }),
        "target_valid",
    ))
}

pub fn run(tokens: &[String]) -> (i32, CliResult) {
    let matches = match command()
        .try_get_matches_from(std::iter::once("tooling".to_owned()).chain(tokens.iter().cloned()))
    {
        Ok(matches) => matches,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            return (0, success(json!({"help":error.to_string()}), "ok"));
        }
        Err(error) => {
            return failure(CliError::new(
                2,
                "cli_usage",
                "The Tooling command is invalid.",
                json!({"error":error.to_string()}),
            ));
        }
    };
    let result = match matches.subcommand() {
        Some(("contract", contract)) => match contract.subcommand() {
            Some(("list", args)) => query::list(optional(args, "scope"), optional(args, "prefix"))
                .map(|data| (data, "ok"))
                .map_err(query_error),
            Some(("describe", args)) => {
                let result = if let Some(path) = optional(args, "command") {
                    if optional(args, "field").is_some() || optional(args, "variant").is_some() {
                        Err(query::QueryError {
                            reason: "invalid_contract_query",
                            message: "--field and --variant require a contract ID.".into(),
                        })
                    } else {
                        query::describe_command(path)
                    }
                } else {
                    query::describe_contract(
                        option(args, "contract_id"),
                        optional(args, "field"),
                        optional(args, "variant"),
                    )
                };
                result.map(|data| (data, "ok")).map_err(query_error)
            }
            Some(("scaffold", args)) => {
                let branches = args
                    .get_many::<String>("branch")
                    .map(|items| items.cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                query::scaffold(option(args, "contract_id"), &branches)
                    .map(|data| (data, "ok"))
                    .map_err(query_error)
            }
            Some(("example", args)) => {
                query::example_query(option(args, "contract_id"), option(args, "scenario"))
                    .map(|data| (data, "ok"))
                    .map_err(query_error)
            }
            _ => unreachable!("clap requires a contract subcommand"),
        },
        Some(("policy", policy)) => match policy.subcommand() {
            Some(("discover", args)) => discover(args).map(|data| (data, "ok")),
            Some(("load", args)) => load(args).map(|data| (data, "ok")),
            _ => unreachable!("clap requires a policy subcommand"),
        },
        Some(("operation", operation)) => match operation.subcommand() {
            Some(("evaluate", args)) => evaluate_operation(args),
            _ => unreachable!("clap requires an operation subcommand"),
        },
        _ => unreachable!("clap requires a command"),
    };
    match result {
        Ok((data, reason)) => (0, success(data, reason)),
        Err(error) => failure(error),
    }
}

#[cfg(test)]
mod metadata_tests {
    use super::*;

    #[test]
    fn every_tooling_leaf_obeys_argument_metadata() {
        fn check(
            node: &Value,
            prefix: Vec<String>,
            inherited: Vec<(usize, usize)>,
            count: &mut usize,
        ) {
            let mut tokens = prefix;
            let mut required = inherited;
            let selected_groups: std::collections::BTreeSet<_> = node["exclusive_groups"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|group| group["required"] == true)
                .filter_map(|group| group["members"].as_array()?.first()?.as_str())
                .collect();
            for argument in node["arguments"].as_array().unwrap() {
                if argument["required"] != true
                    && !selected_groups.contains(argument["name"].as_str().unwrap())
                {
                    continue;
                }
                let start = tokens.len();
                if let Some(flag) = argument["flags"].as_array().unwrap().first() {
                    tokens.push(flag.as_str().unwrap().to_owned());
                }
                if argument["boolean"] != true {
                    let value = argument["choices"]
                        .as_array()
                        .and_then(|items| items.first())
                        .and_then(Value::as_str)
                        .unwrap_or("x");
                    tokens.push(value.to_owned());
                }
                required.push((start, tokens.len()));
            }
            let children = node["children"].as_array().unwrap();
            if children.is_empty() {
                let parse = |args: &[String]| {
                    contract_core::clap_command("tooling")
                        .unwrap()
                        .try_get_matches_from(
                            std::iter::once("tooling".to_owned()).chain(args.iter().cloned()),
                        )
                };
                assert!(parse(&tokens).is_ok(), "{tokens:?}: valid metadata input");
                for (start, end) in required {
                    let mut omitted = tokens.clone();
                    omitted.drain(start..end);
                    assert!(
                        parse(&omitted).is_err(),
                        "{tokens:?}: missing required argument"
                    );
                }
                for argument in node["arguments"].as_array().unwrap() {
                    if !argument["choices"].is_array() {
                        continue;
                    }
                    let Some(flag) = argument["flags"]
                        .as_array()
                        .unwrap()
                        .first()
                        .and_then(Value::as_str)
                    else {
                        continue;
                    };
                    let mut invalid = tokens.clone();
                    if let Some(index) = invalid.iter().position(|token| token == flag) {
                        invalid[index + 1] = "__invalid_choice__".to_owned();
                    } else {
                        invalid.extend([flag.to_owned(), "__invalid_choice__".to_owned()]);
                    }
                    assert!(
                        parse(&invalid).is_err(),
                        "{tokens:?}: invalid choice accepted"
                    );
                }
                for group in node["exclusive_groups"].as_array().unwrap() {
                    let members = group["members"].as_array().unwrap();
                    if members.len() < 2 {
                        continue;
                    }
                    let mut conflicting = tokens.clone();
                    for member in members.iter().take(2) {
                        let argument = node["arguments"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|argument| argument["name"] == *member)
                            .unwrap();
                        if let Some(flag) = argument["flags"]
                            .as_array()
                            .unwrap()
                            .first()
                            .and_then(Value::as_str)
                        {
                            if conflicting.iter().any(|token| token == flag) {
                                continue;
                            }
                            conflicting.push(flag.to_owned());
                            if argument["boolean"] != true {
                                conflicting.push("x".to_owned());
                            }
                        }
                    }
                    assert!(
                        parse(&conflicting).is_err(),
                        "{tokens:?}: exclusive arguments accepted"
                    );
                }
                *count += 1;
            } else {
                for child in children {
                    let mut path = tokens.clone();
                    path.push(child["name"].as_str().unwrap().to_owned());
                    check(child, path, required.clone(), count);
                }
            }
        }
        let tree = contract_core::command_tree("tooling").unwrap();
        let mut count = 0;
        check(&tree["root"], Vec::new(), Vec::new(), &mut count);
        assert_eq!(count, 7);
    }

    #[test]
    fn every_public_tooling_example_matches_metadata_and_applicable_model() {
        fn check_presence<T: serde::de::DeserializeOwned>(
            id: &str,
            scenario: &str,
            description: &Value,
            example: &Value,
        ) {
            for field in description["fields"].as_array().unwrap() {
                let name = field["name"].as_str().unwrap();
                let mut omitted = example.clone();
                omitted.as_object_mut().unwrap().remove(name);
                assert_eq!(
                    serde_json::from_value::<T>(omitted).is_ok(),
                    field["required"] == false,
                    "{id}/{scenario}/{name}: presence differs from metadata"
                );
                if field["type"] == "nullable_object" {
                    let mut explicit_null = example.clone();
                    explicit_null[name] = Value::Null;
                    assert!(
                        serde_json::from_value::<T>(explicit_null).is_ok(),
                        "{id}/{scenario}/{name}: null rejected"
                    );
                }
            }
        }
        fn matches_type(value: &Value, declared: &str) -> bool {
            match declared {
                "nullable_object" => value.is_null() || value.is_object(),
                "string" => value.is_string(),
                "boolean" => value.is_boolean(),
                "integer" => value.is_i64() || value.is_u64(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                "literal" | "any" => true,
                _ => false,
            }
        }
        let mut count = 0;
        for (id, scenarios) in contract_core::examples() {
            if !id.starts_with("tooling-") {
                continue;
            }
            let description = &contract_core::contract(id).unwrap()["description"];
            for (scenario, record) in scenarios.as_object().unwrap() {
                let example = &record["example"];
                let valid = record["valid"].as_bool().unwrap();
                if valid {
                    let fields = example
                        .as_object()
                        .expect("positive Tooling example object");
                    for required in description["required"].as_array().unwrap() {
                        assert!(
                            fields.contains_key(required.as_str().unwrap()),
                            "{id}/{scenario}: required {required}"
                        );
                    }
                    for name in fields.keys() {
                        assert!(
                            description["fields"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|field| field["name"] == name.as_str()),
                            "{id}/{scenario}: unknown {name}"
                        );
                    }
                    for field in description["fields"].as_array().unwrap() {
                        let name = field["name"].as_str().unwrap();
                        if let Some(value) = fields.get(name) {
                            assert!(
                                matches_type(value, field["type"].as_str().unwrap()),
                                "{id}/{scenario}/{name}: type mismatch"
                            );
                        }
                    }
                }
                match id {
                    "tooling-policy-load-request" => {
                        assert_eq!(
                            serde_json::from_value::<PolicyLoadRequest>(example.clone()).is_ok(),
                            valid,
                            "{id}/{scenario}"
                        );
                        if valid {
                            check_presence::<PolicyLoadRequest>(id, scenario, description, example);
                        }
                    }
                    "tooling-operation-evaluate-request" => {
                        assert_eq!(
                            serde_json::from_value::<operation::EvaluateRequest>(example.clone())
                                .is_ok(),
                            valid,
                            "{id}/{scenario}"
                        );
                        if valid {
                            check_presence::<operation::EvaluateRequest>(
                                id,
                                scenario,
                                description,
                                example,
                            );
                        }
                    }
                    "tooling-cli-result" => {
                        assert!(valid);
                        serde_json::from_value::<tooling_model::contract::CliEnvelope>(
                            example.clone(),
                        )
                        .unwrap();
                    }
                    "tooling-policy-discover-response"
                    | "tooling-policy-load-response"
                    | "tooling-operation-evaluate-response" => {
                        assert!(valid, "{id}/{scenario}: response has no request validator");
                    }
                    _ => panic!("{id}/{scenario}: missing example classification"),
                }
                count += 1;
            }
        }
        assert_eq!(count, 10);
    }

    #[test]
    fn policy_load_nullable_metadata_matches_request_deserialization() {
        let description = &contract_core::contract("tooling-policy-load-request")
            .expect("public contract")["description"];
        assert!(
            description["required"]
                .as_array()
                .unwrap()
                .contains(&json!("previous"))
        );
        assert!(
            description["optional"]
                .as_array()
                .unwrap()
                .contains(&json!("semantic_review"))
        );
        let fields = description["fields"].as_array().unwrap();
        let previous = fields
            .iter()
            .find(|field| field["name"] == "previous")
            .unwrap();
        assert_eq!(previous["type"], "nullable_object");
        assert_eq!(previous["required"], true);
        let review = fields
            .iter()
            .find(|field| field["name"] == "semantic_review")
            .unwrap();
        assert_eq!(review["required"], false);
        assert_eq!(review["type"], "nullable_object");

        let base = json!({"selected_paths":[],"applicable_references":[],"previous":null});
        assert!(serde_json::from_value::<PolicyLoadRequest>(base.clone()).is_ok());
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove("previous");
        assert!(serde_json::from_value::<PolicyLoadRequest>(missing).is_err());
        let mut value = base.clone();
        value["previous"] = json!([]);
        assert!(serde_json::from_value::<PolicyLoadRequest>(value).is_err());
        let mut value = base.clone();
        value["semantic_review"] = json!({"reviewed":true,"compatible":true});
        assert!(serde_json::from_value::<PolicyLoadRequest>(value).is_ok());
        let mut value = base;
        value["semantic_review"] = Value::Null;
        assert!(serde_json::from_value::<PolicyLoadRequest>(value).is_ok());
    }
}
