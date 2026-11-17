//! Skill CLI command tree: names, arguments, Help text and the request and
//! response contract each command references.

use contract_core::model::{ArgumentAction, CommandArgument, CommandNode, ExclusiveGroup};
use contract_core::{ContractResult, DomainResources, Registry};
use skill_model::{
    DISCOVER_RESPONSE, DOMAIN, RESOLVE_REQUEST, RESOLVE_RESPONSE, SWITCH_RESPONSE,
    VALIDATE_RESPONSE, contract_examples, contract_records,
};

fn argument(name: &str, required: bool, help: &str) -> CommandArgument {
    CommandArgument {
        name: name.to_owned(),
        flags: vec![format!("--{name}")],
        required,
        action: ArgumentAction::Set,
        choices: Vec::new(),
        nargs_any: false,
        help: help.to_owned(),
    }
}

fn command(name: &str, help: &str, arguments: Vec<CommandArgument>) -> CommandNode {
    CommandNode {
        name: name.to_owned(),
        help: help.to_owned(),
        arguments,
        exclusive_groups: Vec::new(),
        children: Vec::new(),
        request_contract: None,
        response_contract: None,
    }
}

fn commands() -> CommandNode {
    let install = || {
        argument(
            "install",
            true,
            "Installation root holding shared and Skill directories.",
        )
    };
    let mut discover = command(
        "discover",
        "List the direct children of one instruction node.",
        vec![
            install(),
            argument("skill", true, "Skill whose tree is read."),
            argument("node", false, "Parent node path; the Root when omitted."),
        ],
    );
    discover.response_contract = Some(DISCOVER_RESPONSE.to_owned());
    let mut resolve = command(
        "resolve",
        "Validate a complete candidate set and list its sources.",
        vec![
            install(),
            argument(
                "request",
                true,
                "Request file path, or - for standard input.",
            ),
        ],
    );
    resolve.request_contract = Some(RESOLVE_REQUEST.to_owned());
    resolve.response_contract = Some(RESOLVE_RESPONSE.to_owned());
    let mut switch = command(
        "switch",
        "Validate both candidates and plan the incremental replacement.",
        vec![
            install(),
            argument("active", true, "Request file of the active set."),
            argument("next", true, "Request file of the next set."),
        ],
    );
    switch.request_contract = Some(RESOLVE_REQUEST.to_owned());
    switch.response_contract = Some(SWITCH_RESPONSE.to_owned());
    let mut validate = command(
        "validate",
        "Validate every source of an installation.",
        vec![install()],
    );
    validate.response_contract = Some(VALIDATE_RESPONSE.to_owned());

    let mut scope = argument("scope", false, "Entries to list.");
    scope.choices = ["all", "commands", "contracts"].map(str::to_owned).into();
    let list = command(
        "list",
        "List commands and contracts of this CLI.",
        vec![
            scope,
            argument("prefix", false, "Keep entries starting with this text."),
        ],
    );
    let mut describe = command(
        "describe",
        "Describe one command, contract or contract field.",
        vec![
            argument("command", false, "Command path, such as \"skill resolve\"."),
            argument("contract", false, "Contract identifier."),
            argument(
                "field",
                false,
                "RFC 6901 field pointer inside the contract.",
            ),
            argument("variant", false, "Branch variant of the field."),
        ],
    );
    describe.exclusive_groups = vec![ExclusiveGroup {
        members: vec!["command".to_owned(), "contract".to_owned()],
        required: true,
    }];
    let mut branch = argument("branch", false, "Branch selection as <pointer>=<variant>.");
    branch.action = ArgumentAction::Append;
    let scaffold = command(
        "scaffold",
        "Derive the structural scaffold of a request contract.",
        vec![argument("contract", true, "Contract identifier."), branch],
    );
    let example = command(
        "example",
        "Show a declared example of a contract.",
        vec![
            argument("contract", true, "Contract identifier."),
            argument("scenario", true, "Example scenario name."),
        ],
    );
    let mut root = command("skill", "Runtime skill instruction resolver.", Vec::new());
    root.children = vec![
        discover, resolve, switch, validate, list, describe, scaffold, example,
    ];
    root
}

/// Assemble and structurally validate the Skill CLI registry.
pub fn registry() -> ContractResult<Registry> {
    Registry::build(DomainResources {
        domain: DOMAIN.to_owned(),
        root_commands: vec![commands()],
        contracts: contract_records(),
        examples: contract_examples(),
    })
}
