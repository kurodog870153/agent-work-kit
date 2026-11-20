//! Build a `clap` command tree from a domain's declared command tree.
//!
//! The tree is data, so Parser and Help stay derived from the same declaration
//! rather than being maintained twice.

use clap::builder::PossibleValuesParser;
use clap::{Arg, ArgAction, ArgGroup, Command};

use crate::model::{ArgumentAction, CommandNode};

/// Build the root `clap` command for `node`.
pub fn build(node: &CommandNode) -> Command {
    let mut command = Command::new(node.name.clone())
        .about(node.help.clone())
        .subcommand_required(!node.children.is_empty())
        .disable_help_subcommand(true);
    for argument in &node.arguments {
        let mut item = Arg::new(argument.name.clone()).required(argument.required);
        for flag in &argument.flags {
            if let Some(long) = flag.strip_prefix("--") {
                item = item.long(long.to_owned());
            } else if let Some(short) = flag.strip_prefix('-')
                && let Some(first) = short.chars().next()
            {
                item = item.short(first);
            }
        }
        item = match argument.action {
            ArgumentAction::SetTrue => item.action(ArgAction::SetTrue),
            ArgumentAction::Append => item.action(ArgAction::Append),
            ArgumentAction::Set => item.action(ArgAction::Set),
        };
        if argument.nargs_any {
            item = item.num_args(0..);
        }
        if !argument.choices.is_empty() {
            item = item.value_parser(PossibleValuesParser::new(argument.choices.clone()));
        }
        if !argument.help.is_empty() {
            item = item.help(argument.help.clone());
        }
        command = command.arg(item);
    }
    for (index, group) in node.exclusive_groups.iter().enumerate() {
        command = command.group(
            ArgGroup::new(format!("exclusive_{index}"))
                .args(group.members.clone())
                .required(group.required)
                .multiple(false),
        );
    }
    for child in &node.children {
        command = command.subcommand(build(child));
    }
    command
}
