//! Work CLI: public entry, input/output boundary and dependency assembly.
//!
//! The command tree, Help and parsing all derive from the contract registry
//! this crate assembles from the command tree and the `work-model` contracts,
//! so the public contract exists exactly once.

use std::fs;
use std::io::Read;

use clap::ArgMatches;
use contract_core::clap_tree;
use contract_core::query::{self, ListScope};
use serde::Serialize;
use serde::de::DeserializeOwned;
use work_infrastructure::FsStore;
use work_model::{
    Envelope, ErrorData, Reason, RecordId, RecordKind, SourceContent, SourceCreateRequest,
    WorkError,
};

mod commands;

pub use commands::registry;

/// Exit code and captured output of one invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub code: u8,
    pub stdout: String,
    pub stderr: String,
}

const OK: u8 = 0;
const FAILED: u8 = 1;
const USAGE: u8 = 2;

fn emit<T: Serialize>(envelope: &Envelope<T>) -> Output {
    let (code, text) = match serde_json::to_string_pretty(envelope) {
        Ok(text) => (
            if matches!(envelope, Envelope::Ok { .. }) {
                OK
            } else {
                FAILED
            },
            text,
        ),
        Err(error) => (
            FAILED,
            format!(
                "{{\"status\":\"error\",\"error\":{{\"reason\":\"serialize\",\"message\":{error:?}}}}}"
            ),
        ),
    };
    Output {
        code,
        stdout: format!("{text}\n"),
        stderr: String::new(),
    }
}

fn success<T: Serialize>(data: T) -> Output {
    emit(&Envelope::Ok { data })
}

fn failure(error: ErrorData) -> Output {
    emit::<()>(&Envelope::Error { error })
}

fn work_failure(error: &WorkError) -> Output {
    failure(ErrorData {
        reason: error.reason.code().to_owned(),
        message: error.message.clone(),
    })
}

fn contract_failure(error: &contract_core::ContractError) -> Output {
    failure(ErrorData {
        reason: error.reason().to_owned(),
        message: error.message().to_owned(),
    })
}

fn text<'a>(matches: &'a ArgMatches, name: &str) -> Option<&'a str> {
    matches.get_one::<String>(name).map(String::as_str)
}

fn required<'a>(matches: &'a ArgMatches, name: &str) -> Result<&'a str, WorkError> {
    text(matches, name)
        .ok_or_else(|| WorkError::new(Reason::InvalidRequest, format!("--{name} is required")))
}

fn record_id(matches: &ArgMatches) -> Result<RecordId, WorkError> {
    Ok(RecordId::parse(required(matches, "id")?)?)
}

fn record_kind(matches: &ArgMatches) -> Result<RecordKind, WorkError> {
    let code = required(matches, "kind")?;
    RecordKind::parse(code)
        .filter(|kind| *kind != RecordKind::Correction)
        .ok_or_else(|| WorkError::new(Reason::InvalidRequest, format!("unknown --kind: {code}")))
}

fn version(matches: &ArgMatches) -> Result<Option<u32>, WorkError> {
    text(matches, "version")
        .map(|value| {
            value
                .parse::<u32>()
                .ok()
                .filter(|version| *version > 0)
                .ok_or_else(|| {
                    WorkError::new(
                        Reason::InvalidRequest,
                        "--version must be a positive integer",
                    )
                })
        })
        .transpose()
}

fn read_request<T: DeserializeOwned>(
    path: &str,
    stdin: &mut dyn FnMut() -> String,
) -> Result<T, WorkError> {
    let body = if path == "-" {
        stdin()
    } else {
        fs::read_to_string(path).map_err(|error| {
            WorkError::new(
                Reason::InvalidRequest,
                format!("request file {path}: {error}"),
            )
        })?
    };
    serde_json::from_str(&body)
        .map_err(|error| WorkError::new(Reason::InvalidRequest, error.to_string()))
}

/// Run the CLI with explicit arguments (first element is the program name).
pub fn execute(args: &[String], stdin: &mut dyn FnMut() -> String) -> Output {
    let registry = match registry() {
        Ok(registry) => registry,
        Err(error) => return contract_failure(&error),
    };
    let Some(root) = registry.command("work") else {
        return failure(ErrorData {
            reason: "missing_root_command".to_owned(),
            message: "the work command is not declared".to_owned(),
        });
    };
    let command = clap_tree::build(&root.node)
        .version(env!("CARGO_PKG_VERSION"))
        .disable_help_subcommand(true);
    let matches = match command.try_get_matches_from(args) {
        Ok(matches) => matches,
        Err(error) => {
            let rendered = error.render().to_string();
            return if error.use_stderr() {
                Output {
                    code: USAGE,
                    stdout: String::new(),
                    stderr: rendered,
                }
            } else {
                Output {
                    code: OK,
                    stdout: rendered,
                    stderr: String::new(),
                }
            };
        }
    };
    let Some((name, sub)) = matches.subcommand() else {
        return work_failure(&WorkError::new(
            Reason::InvalidRequest,
            "a command is required",
        ));
    };
    match name {
        "list" => {
            let scope = match text(sub, "scope") {
                Some("commands") => ListScope::Commands,
                Some("contracts") => ListScope::Contracts,
                _ => ListScope::All,
            };
            success(query::list(&registry, scope, text(sub, "prefix")))
        }
        "describe" => match (
            text(sub, "command"),
            text(sub, "contract"),
            text(sub, "field"),
        ) {
            (Some(path), _, _) => query::describe_command(&registry, path)
                .map_or_else(|error| contract_failure(&error), success),
            (None, Some(id), Some(field)) => {
                query::describe_field(&registry, id, field, text(sub, "variant"))
                    .map_or_else(|error| contract_failure(&error), success)
            }
            (None, Some(id), None) => query::describe_contract(&registry, id)
                .map_or_else(|error| contract_failure(&error), success),
            (None, None, _) => work_failure(&WorkError::new(
                Reason::InvalidRequest,
                "describe needs --command or --contract",
            )),
        },
        "scaffold" => {
            let branches: Vec<String> = sub
                .get_many::<String>("branch")
                .map(|values| values.cloned().collect())
                .unwrap_or_default();
            match required(sub, "contract") {
                Ok(id) => query::scaffold_query(&registry, id, &branches)
                    .map_or_else(|error| contract_failure(&error), success),
                Err(error) => work_failure(&error),
            }
        }
        "example" => match (required(sub, "contract"), required(sub, "scenario")) {
            (Ok(id), Ok(scenario)) => query::example_query(&registry, id, scenario)
                .map_or_else(|error| contract_failure(&error), success),
            (Err(error), _) | (_, Err(error)) => work_failure(&error),
        },
        _ => run_record_command(name, sub, stdin).unwrap_or_else(|error| work_failure(&error)),
    }
}

fn run_record_command(
    name: &str,
    sub: &ArgMatches,
    stdin: &mut dyn FnMut() -> String,
) -> Result<Output, WorkError> {
    match name {
        "status" => Ok(success(work_flow::status(&store(sub)?)?)),
        "recover" => Ok(success(work_flow::recover(&store(sub)?)?)),
        "trace" => Ok(success(work_flow::trace(
            &store(sub)?,
            record_kind(sub)?,
            &record_id(sub)?,
        )?)),
        group => {
            let Some((leaf, leaf_matches)) = sub.subcommand() else {
                return Err(WorkError::new(
                    Reason::InvalidRequest,
                    format!("unknown command: {group}"),
                ));
            };
            let store = store(leaf_matches)?;
            let request = || required(leaf_matches, "request");
            Ok(match (group, leaf) {
                ("source", "create") => {
                    let mut source: SourceCreateRequest = read_request(request()?, stdin)?;
                    if let Some(path) = text(leaf_matches, "from-file") {
                        if source.content.is_some() {
                            return Err(WorkError::new(
                                Reason::InvalidRequest,
                                "--from-file requires the request to omit content",
                            ));
                        }
                        let bytes = fs::read(path).map_err(|error| {
                            WorkError::new(
                                Reason::InvalidRequest,
                                format!("cannot read Source attachment file {path}: {error}"),
                            )
                        })?;
                        source.content = Some(SourceContent::Binary { bytes });
                    }
                    success(work_flow::source_create(&store, source)?)
                }
                ("source", "show") => success(work_flow::source_show(
                    &store,
                    &record_id(leaf_matches)?,
                    version(leaf_matches)?,
                )?),
                ("discussion", "create") => success(work_flow::discussion_create(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("discussion", "round") => success(work_flow::discussion_round(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("discussion", "show") => success(work_flow::discussion_show(
                    &store,
                    &record_id(leaf_matches)?,
                    version(leaf_matches)?,
                )?),
                ("discussion", "history") => success(work_flow::discussion_history(
                    &store,
                    &record_id(leaf_matches)?,
                )?),
                ("discussion", "progress") => success(work_flow::discussion_progress(
                    &store,
                    &record_id(leaf_matches)?,
                )?),
                ("correction", "preview") => success(work_flow::correction_preview(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("correction", "apply") => success(work_flow::correction_apply(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("correction", "list") => success(work_flow::correction_list(
                    &store,
                    record_kind(leaf_matches)?,
                    &record_id(leaf_matches)?,
                )?),
                ("task", "preview") => success(work_flow::task_preview(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("task", "apply") => success(work_flow::task_apply(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("task", "complete") => success(work_flow::task_complete(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("task", "show") => success(work_flow::task_show(
                    &store,
                    &record_id(leaf_matches)?,
                    version(leaf_matches)?,
                )?),
                ("execution", "record") => success(work_flow::execution_record(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("execution", "show") => success(work_flow::execution_show(
                    &store,
                    &record_id(leaf_matches)?,
                    version(leaf_matches)?,
                )?),
                ("acceptance", "task-record") => success(work_flow::task_acceptance_record(
                    &store,
                    read_request(request()?, stdin)?,
                )?),
                ("acceptance", "task-show") => success(work_flow::task_acceptance_show(
                    &store,
                    &record_id(leaf_matches)?,
                    version(leaf_matches)?,
                )?),
                ("acceptance", "requirement-record") => {
                    success(work_flow::requirement_acceptance_record(
                        &store,
                        read_request(request()?, stdin)?,
                    )?)
                }
                ("acceptance", "requirement-show") => {
                    success(work_flow::requirement_acceptance_show(
                        &store,
                        &record_id(leaf_matches)?,
                        version(leaf_matches)?,
                    )?)
                }
                _ => {
                    return Err(WorkError::new(
                        Reason::InvalidRequest,
                        format!("unknown command: {group} {leaf}"),
                    ));
                }
            })
        }
    }
}

fn store(matches: &ArgMatches) -> Result<FsStore, WorkError> {
    Ok(FsStore::open(required(matches, "root")?))
}

/// Read all of standard input; used only when a request path is `-`.
pub fn read_stdin() -> String {
    let mut body = String::new();
    let _ = std::io::stdin().read_to_string(&mut body);
    body
}

#[cfg(test)]
mod tests;
