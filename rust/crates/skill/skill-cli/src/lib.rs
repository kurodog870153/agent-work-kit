//! Skill CLI: instruction discovery, resolution, validation and composition.
//!
//! The command tree, Help and parsing all derive from the contract registry
//! this crate assembles from the command tree and the `skill-model` contracts,
//! so the public contract exists exactly once.

use std::fs;
use std::io::Read;

use clap::ArgMatches;
use contract_core::clap_tree;
use contract_core::query::{self, ListScope};
use serde::Serialize;
use skill_infrastructure::FsStore;
use skill_model::{Envelope, ErrorData, Reason, RequestDto, SkillError};

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

fn skill_failure(error: &SkillError) -> Output {
    failure(ErrorData {
        reason: error.reason.code().to_owned(),
        message: error.message.clone(),
        source: error.source.clone(),
        scope: error.scope.map(|scope| scope.code().to_owned()),
        selection: error.selection.clone(),
    })
}

fn contract_failure(error: &contract_core::ContractError) -> Output {
    failure(ErrorData {
        reason: error.reason().to_owned(),
        message: error.message().to_owned(),
        source: None,
        scope: None,
        selection: None,
    })
}

fn text<'a>(matches: &'a ArgMatches, name: &str) -> Option<&'a str> {
    matches.get_one::<String>(name).map(String::as_str)
}

fn required<'a>(matches: &'a ArgMatches, name: &str) -> Result<&'a str, SkillError> {
    text(matches, name)
        .ok_or_else(|| SkillError::new(Reason::InvalidRequest, format!("--{name} is required")))
}

fn read_request(path: &str, stdin: &mut dyn FnMut() -> String) -> Result<RequestDto, SkillError> {
    let body = if path == "-" {
        stdin()
    } else {
        fs::read_to_string(path).map_err(|error| {
            SkillError::new(Reason::Unreadable, format!("request file {path}: {error}"))
        })?
    };
    serde_json::from_str(&body)
        .map_err(|error| SkillError::new(Reason::InvalidRequest, error.to_string()))
}

/// Run the CLI with explicit arguments (first element is the program name).
pub fn execute(args: &[String], stdin: &mut dyn FnMut() -> String) -> Output {
    let registry = match registry() {
        Ok(registry) => registry,
        Err(error) => return contract_failure(&error),
    };
    let Some(root) = registry.command("skill") else {
        return failure(ErrorData {
            reason: "missing_root_command".to_owned(),
            message: "the skill command is not declared".to_owned(),
            source: None,
            scope: None,
            selection: None,
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
        return skill_failure(&SkillError::new(
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
            (None, None, _) => skill_failure(&SkillError::new(
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
                Err(error) => skill_failure(&error),
            }
        }
        "example" => match (required(sub, "contract"), required(sub, "scenario")) {
            (Ok(id), Ok(scenario)) => query::example_query(&registry, id, scenario)
                .map_or_else(|error| contract_failure(&error), success),
            (Err(error), _) | (_, Err(error)) => skill_failure(&error),
        },
        _ => run_runtime(name, sub, stdin),
    }
}

fn run_runtime(name: &str, sub: &ArgMatches, stdin: &mut dyn FnMut() -> String) -> Output {
    let outcome = (|| -> Result<Output, SkillError> {
        let store = FsStore::open(required(sub, "install")?)?;
        match name {
            "discover" => Ok(success(skill_flow::discover(
                &store,
                required(sub, "skill")?,
                text(sub, "node").unwrap_or(""),
            )?)),
            "resolve" => {
                let request = read_request(required(sub, "request")?, stdin)?;
                Ok(success(skill_flow::resolve(&store, request)?))
            }
            "switch" => {
                let active = read_request(required(sub, "active")?, stdin)?;
                let next = read_request(required(sub, "next")?, stdin)?;
                Ok(success(skill_flow::switch(&store, active, next)?))
            }
            "validate" => {
                let data = skill_flow::validate(&store)?;
                let code = if data.valid { OK } else { FAILED };
                let mut output = success(data);
                output.code = code;
                Ok(output)
            }
            other => Err(SkillError::new(
                Reason::InvalidRequest,
                format!("unknown command: {other}"),
            )),
        }
    })();
    outcome.unwrap_or_else(|error| skill_failure(&error))
}

/// Read all of standard input; used only when a request path is `-`.
pub fn read_stdin() -> String {
    let mut body = String::new();
    let _ = std::io::stdin().read_to_string(&mut body);
    body
}
