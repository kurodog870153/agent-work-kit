//! Shared, build-validated index of public CLI contract resources.

pub mod payload;
pub mod query;
pub mod validation;

use std::collections::BTreeMap;
use std::sync::OnceLock;

use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, ArgGroup, Command};
use serde_json::Value;

static REGISTRY_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/registry.json"));

pub fn registry() -> &'static Value {
    static REGISTRY: OnceLock<Value> = OnceLock::new();
    REGISTRY.get_or_init(|| serde_json::from_str(REGISTRY_JSON).expect("validated registry"))
}

pub fn command(path: &str) -> Option<&'static Value> {
    registry()["commands"].get(path)
}

pub fn command_tree(cli: &str) -> Option<&'static Value> {
    registry()["command_trees"].get(cli)
}

pub fn clap_command(cli: &str) -> Option<Command> {
    command_tree(cli).map(|tree| build_command(&tree["root"]))
}

fn build_command(node: &Value) -> Command {
    let name = node["name"].as_str().expect("command name");
    let mut command = Command::new(name.to_owned())
        .about(node["help"].as_str().expect("command help").to_owned())
        .subcommand_required(!node["children"].as_array().expect("children").is_empty());
    for item in node["arguments"].as_array().expect("arguments") {
        let name = item["name"].as_str().expect("argument name");
        let mut argument =
            Arg::new(name.to_owned()).required(item["required"].as_bool().expect("required"));
        for flag in item["flags"].as_array().expect("flags") {
            let flag = flag.as_str().expect("flag string");
            if let Some(long) = flag.strip_prefix("--") {
                argument = argument.long(long.to_owned());
            } else if let Some(short) = flag.strip_prefix('-') {
                argument = argument.short(short.chars().next().expect("short flag"));
            }
        }
        argument = if item["boolean"] == true {
            argument.action(ArgAction::SetTrue)
        } else if item["append"] == true {
            argument.action(ArgAction::Append)
        } else {
            argument.action(ArgAction::Set)
        };
        if item["nargs"] == "*" {
            argument = argument.num_args(0..);
        }
        if let Some(choices) = item["choices"].as_array() {
            argument = argument.value_parser(PossibleValuesParser::new(
                choices
                    .iter()
                    .map(|choice| choice.as_str().expect("choice").to_owned())
                    .collect::<Vec<_>>(),
            ));
        }
        if let Some(help) = item["help"].as_str() {
            argument = argument.help(help.to_owned());
        }
        command = command.arg(argument);
    }
    for (index, group) in node["exclusive_groups"]
        .as_array()
        .expect("groups")
        .iter()
        .enumerate()
    {
        command = command.group(
            ArgGroup::new(format!("exclusive_{index}"))
                .args(
                    group["members"]
                        .as_array()
                        .expect("group members")
                        .iter()
                        .map(|m| m.as_str().expect("member").to_owned())
                        .collect::<Vec<_>>(),
                )
                .required(group["required"].as_bool().expect("group required"))
                .multiple(false),
        );
    }
    for child in node["children"].as_array().expect("children") {
        command = command.subcommand(build_command(child));
    }
    command
}

pub fn contract(id: &str) -> Option<&'static Value> {
    registry()["contracts"].get(id)
}

pub fn query_data_schema(id: &str) -> Option<&'static Value> {
    registry()["query_data_schemas"].get(id)
}

pub fn example(id: &str, scenario: &str) -> Option<&'static Value> {
    registry()["examples"].get(id)?.get(scenario)
}

pub fn command_paths() -> Vec<&'static str> {
    registry()["commands"]
        .as_object()
        .expect("command index")
        .keys()
        .map(String::as_str)
        .collect()
}

pub fn contract_ids() -> Vec<&'static str> {
    registry()["contracts"]
        .as_object()
        .expect("contract index")
        .keys()
        .map(String::as_str)
        .collect()
}

pub fn examples() -> BTreeMap<&'static str, &'static Value> {
    registry()["examples"]
        .as_object()
        .expect("example index")
        .iter()
        .map(|(name, value)| (name.as_str(), value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn existing_public_resources_are_indexed() {
        assert!(command("work contract list").is_some());
        assert!(contract("work-cli-result").is_some());
        assert!(example("tooling-policy-load-request", "first").is_some());
        assert_eq!(command_paths().len(), 121);
        assert_eq!(contract_ids().len(), 98);
        assert_eq!(examples().len(), 93);
        for id in [
            "work-spec-migration-preview-request",
            "work-spec-prepare",
            "work-spec-reconciliation-preview-request",
            "work-spec-transaction",
            "work-spec-update",
        ] {
            assert!(example(id, "default").is_none(), "{id} is fixture-only");
            assert!(contract(id).is_some(), "{id} contract remains public");
        }
    }
}
