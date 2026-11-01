//! Stateless Tooling command boundary.

use std::cell::RefCell;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use clap::{Arg, ArgAction, ArgMatches, Command};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tooling_flow::operation;
use tooling_flow::policy::{self, PolicyLoadError, PolicySummary};

#[derive(Debug, Serialize)]
pub struct CliResult {
    pub schema: &'static str,
    pub status: &'static str,
    pub reason_code: String,
    pub message: &'static str,
    pub data: Value,
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
        status: "success",
        reason_code: reason_code.into(),
        message: "The command completed successfully.",
        data,
    }
}

fn failure(error: CliError) -> (i32, CliResult) {
    (
        error.code,
        CliResult {
            schema: "tooling-cli-result",
            status: if error.code == 8 || error.code == 10 {
                "failed"
            } else {
                "rejected"
            },
            reason_code: error.reason.into(),
            message: error.message,
            data: error.data,
        },
    )
}

fn required(name: &'static str) -> Arg {
    Arg::new(name)
        .long(name)
        .required(true)
        .action(ArgAction::Set)
}

fn command() -> Command {
    Command::new("tooling")
        .version(env!("CARGO_PKG_VERSION"))
        .subcommand_required(true)
        .subcommand(
            Command::new("policy")
                .subcommand_required(true)
                .subcommand(
                    Command::new("discover")
                        .arg(required("skill-root"))
                        .arg(required("node")),
                )
                .subcommand(
                    Command::new("load")
                        .arg(required("skill-root"))
                        .arg(required("input-file")),
                ),
        )
        .subcommand(
            Command::new("operation")
                .subcommand_required(true)
                .subcommand(Command::new("evaluate").arg(required("input-file"))),
        )
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
    previous: Option<PolicySummary>,
    #[serde(default)]
    semantic_review: Option<SemanticReview>,
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
