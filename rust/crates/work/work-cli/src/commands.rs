//! Work CLI command tree: names, arguments, Help text and the request and
//! response contract each command references.

use contract_core::model::{ArgumentAction, CommandArgument, CommandNode, ExclusiveGroup};
use contract_core::{ContractResult, DomainResources, Registry};
use work_model::{
    APPLY_REQUEST_SCHEMA, CORRECTION_LIST_RESPONSE, CORRECTION_PREVIEW_RESPONSE,
    DISCUSSION_CREATE_SCHEMA, DISCUSSION_RESPONSE, DISCUSSION_ROUND_SCHEMA, DOMAIN,
    DRAFT_REQUEST_SCHEMA, EXECUTION_RECORD_SCHEMA, EXECUTION_RESPONSE, HISTORY_RESPONSE,
    PROGRESS_RESPONSE, REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA, REQUIREMENT_ACCEPTANCE_RESPONSE,
    SOURCE_CREATE_SCHEMA, SOURCE_RESPONSE, STATUS_RESPONSE, TASK_ACCEPTANCE_RECORD_SCHEMA,
    TASK_ACCEPTANCE_RESPONSE, TASK_APPLY_SCHEMA, TASK_COMPLETE_SCHEMA, TASK_PREVIEW_RESPONSE,
    TASK_PREVIEW_SCHEMA, TASK_RESPONSE, TRACE_RESPONSE, WRITE_RESPONSE, contract_examples,
    contract_records,
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

fn root_argument() -> CommandArgument {
    argument("root", true, "Work data root directory.")
}

fn request_argument() -> CommandArgument {
    argument(
        "request",
        true,
        "Request file path, or - for standard input.",
    )
}

fn id_argument(help: &str) -> CommandArgument {
    argument("id", true, help)
}

fn version_argument() -> CommandArgument {
    argument(
        "version",
        false,
        "Published version; the latest when omitted.",
    )
}

fn leaf(
    name: &str,
    help: &str,
    arguments: Vec<CommandArgument>,
    request: Option<&str>,
    response: &str,
) -> CommandNode {
    let mut node = command(name, help, arguments);
    node.request_contract = request.map(str::to_owned);
    node.response_contract = Some(response.to_owned());
    node
}

fn group(name: &str, help: &str, children: Vec<CommandNode>) -> CommandNode {
    let mut node = command(name, help, Vec::new());
    node.children = children;
    node
}

fn commands() -> CommandNode {
    let mut kind = argument("kind", true, "Kind of the corrected record.");
    kind.choices = [
        "source",
        "discussion",
        "task",
        "execution",
        "task_acceptance",
        "requirement_acceptance",
    ]
    .map(str::to_owned)
    .into();
    let mut trace_kind = argument("kind", true, "Kind of the record to trace from.");
    trace_kind.choices = [
        "source",
        "discussion",
        "task",
        "execution",
        "task_acceptance",
        "requirement_acceptance",
    ]
    .map(str::to_owned)
    .into();
    let mut scope = argument("scope", false, "Entries to list.");
    scope.choices = ["all", "commands", "contracts"].map(str::to_owned).into();
    let mut branch = argument("branch", false, "Branch selection as <pointer>=<variant>.");
    branch.action = ArgumentAction::Append;

    let mut root = command("work", "Work formal record manager.", Vec::new());
    root.children = vec![
        group(
            "source",
            "Original external evidence.",
            vec![
                leaf(
                    "create",
                    "Save a Source.",
                    vec![
                        root_argument(),
                        request_argument(),
                        argument(
                            "from-file",
                            false,
                            "Read raw attachment bytes; omit content from the request.",
                        ),
                    ],
                    Some(SOURCE_CREATE_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "show",
                    "Read a Source with its links and corrections.",
                    vec![
                        root_argument(),
                        id_argument("Source ID."),
                        version_argument(),
                    ],
                    None,
                    SOURCE_RESPONSE,
                ),
            ],
        ),
        group(
            "discussion",
            "Requirement discussion records.",
            vec![
                leaf(
                    "create",
                    "Open a Discussion from the original request.",
                    vec![root_argument(), request_argument()],
                    Some(DISCUSSION_CREATE_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "round",
                    "Record one round as a new Discussion version.",
                    vec![root_argument(), request_argument()],
                    Some(DISCUSSION_ROUND_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "show",
                    "Read one Discussion version.",
                    vec![
                        root_argument(),
                        id_argument("Discussion ID."),
                        version_argument(),
                    ],
                    None,
                    DISCUSSION_RESPONSE,
                ),
                leaf(
                    "history",
                    "Summarise every Discussion version.",
                    vec![root_argument(), id_argument("Discussion ID.")],
                    None,
                    HISTORY_RESPONSE,
                ),
                leaf(
                    "progress",
                    "Report open question, pending items and decisions.",
                    vec![root_argument(), id_argument("Discussion ID.")],
                    None,
                    PROGRESS_RESPONSE,
                ),
            ],
        ),
        group(
            "correction",
            "Additive fixes of published records.",
            vec![
                leaf(
                    "preview",
                    "Build the candidate to review; writes nothing.",
                    vec![root_argument(), request_argument()],
                    Some(DRAFT_REQUEST_SCHEMA),
                    CORRECTION_PREVIEW_RESPONSE,
                ),
                leaf(
                    "apply",
                    "Save a Correction bound to the reviewed candidate.",
                    vec![root_argument(), request_argument()],
                    Some(APPLY_REQUEST_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "list",
                    "List Corrections of one record.",
                    vec![root_argument(), kind, id_argument("Corrected record ID.")],
                    None,
                    CORRECTION_LIST_RESPONSE,
                ),
            ],
        ),
        group(
            "task",
            "TASK records: goal, requirements, constraints, decisions and acceptance criteria.",
            vec![
                leaf(
                    "preview",
                    "Review a TASK creation or revision; does not write any formal record.",
                    vec![root_argument(), request_argument()],
                    Some(TASK_PREVIEW_SCHEMA),
                    TASK_PREVIEW_RESPONSE,
                ),
                leaf(
                    "apply",
                    "Apply exactly the reviewed TASK candidate with version-bound approval.",
                    vec![root_argument(), request_argument()],
                    Some(TASK_APPLY_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "complete",
                    "Complete a TASK once every required criterion has a passing conclusion.",
                    vec![root_argument(), request_argument()],
                    Some(TASK_COMPLETE_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "show",
                    "Read one TASK version with criterion states and links.",
                    vec![root_argument(), id_argument("TASK ID."), version_argument()],
                    None,
                    TASK_RESPONSE,
                ),
            ],
        ),
        group(
            "execution",
            "Execution records and their attempts.",
            vec![
                leaf(
                    "record",
                    "Create an Execution or add an attempt to it.",
                    vec![root_argument(), request_argument()],
                    Some(EXECUTION_RECORD_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "show",
                    "Read one Execution version.",
                    vec![
                        root_argument(),
                        id_argument("Execution ID."),
                        version_argument(),
                    ],
                    None,
                    EXECUTION_RESPONSE,
                ),
            ],
        ),
        group(
            "acceptance",
            "Acceptance conclusions for TASKs and for the requirement.",
            vec![
                leaf(
                    "task-record",
                    "Save TASK Acceptance conclusions reported by the Host or a named user.",
                    vec![root_argument(), request_argument()],
                    Some(TASK_ACCEPTANCE_RECORD_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "task-show",
                    "Read TASK Acceptance with current criterion states.",
                    vec![
                        root_argument(),
                        id_argument("TASK Acceptance ID."),
                        version_argument(),
                    ],
                    None,
                    TASK_ACCEPTANCE_RESPONSE,
                ),
                leaf(
                    "requirement-record",
                    "Save a Requirement Acceptance conclusion for a Discussion.",
                    vec![root_argument(), request_argument()],
                    Some(REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA),
                    WRITE_RESPONSE,
                ),
                leaf(
                    "requirement-show",
                    "Read Requirement Acceptance with its current state.",
                    vec![
                        root_argument(),
                        id_argument("Requirement Acceptance ID."),
                        version_argument(),
                    ],
                    None,
                    REQUIREMENT_ACCEPTANCE_RESPONSE,
                ),
            ],
        ),
        leaf(
            "trace",
            "Trace Source, Discussion, TASK, Execution and Acceptance links around one record.",
            vec![root_argument(), trace_kind, id_argument("Record ID.")],
            None,
            TRACE_RESPONSE,
        ),
        leaf(
            "status",
            "List every record with its latest version.",
            vec![root_argument()],
            None,
            STATUS_RESPONSE,
        ),
        command(
            "list",
            "List commands and contracts of this CLI.",
            vec![
                scope,
                argument("prefix", false, "Keep entries starting with this text."),
            ],
        ),
        {
            let mut describe = command(
                "describe",
                "Describe one command, contract or contract field.",
                vec![
                    argument(
                        "command",
                        false,
                        "Command path, such as \"work source create\".",
                    ),
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
            describe
        },
        command(
            "scaffold",
            "Derive the structural scaffold of a request contract.",
            vec![argument("contract", true, "Contract identifier."), branch],
        ),
        command(
            "example",
            "Show a declared example of a contract.",
            vec![
                argument("contract", true, "Contract identifier."),
                argument("scenario", true, "Example scenario name."),
            ],
        ),
    ];
    root
}

/// Assemble and structurally validate the Work CLI registry.
pub fn registry() -> ContractResult<Registry> {
    Registry::build(DomainResources {
        domain: DOMAIN.to_owned(),
        root_commands: vec![commands()],
        contracts: contract_records(),
        examples: contract_examples(),
    })
}
