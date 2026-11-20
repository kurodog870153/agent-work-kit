//! Behaviour tests of the public contract-core surface.
//!
//! All data is built in test code; there are no fixture files.

use std::collections::BTreeMap;

use contract_core::model::{
    ArgumentAction, CommandArgument, CommandNode, ContractDescription, ContractKind,
    ContractRecord, ContractValue, Discriminator, Example, ExclusiveGroup, FieldDescription,
    FieldType, SchemaNode,
};
use contract_core::query::{self, ListScope};
use contract_core::{DomainResources, Registry};

fn field(name: &str, required: bool) -> FieldDescription {
    FieldDescription {
        name: name.to_owned(),
        field_type: FieldType::String,
        nullable: false,
        required,
        reference: None,
        constant: None,
        schema: None,
    }
}

fn description(id: &str, fields: Vec<FieldDescription>) -> ContractDescription {
    let required = fields
        .iter()
        .filter(|item| item.required)
        .map(|item| item.name.clone())
        .collect();
    let optional = fields
        .iter()
        .filter(|item| !item.required)
        .map(|item| item.name.clone())
        .collect();
    let canonical_order = fields.iter().map(|item| item.name.clone()).collect();
    ContractDescription {
        id: id.to_owned(),
        kind: ContractKind::SemanticRequest,
        caller_constructible: true,
        required,
        optional,
        canonical_order,
        fields,
        example: None,
    }
}

fn record(id: &str, fields: Vec<FieldDescription>) -> ContractRecord {
    ContractRecord {
        description: description(id, fields),
        definitions: BTreeMap::new(),
    }
}

fn argument(name: &str, flag: &str) -> CommandArgument {
    CommandArgument {
        name: name.to_owned(),
        flags: vec![flag.to_owned()],
        required: false,
        action: ArgumentAction::Set,
        choices: Vec::new(),
        nargs_any: false,
        help: String::new(),
    }
}

fn command(name: &str) -> CommandNode {
    CommandNode {
        name: name.to_owned(),
        help: format!("{name} help"),
        arguments: Vec::new(),
        exclusive_groups: Vec::new(),
        children: Vec::new(),
        request_contract: None,
        response_contract: None,
    }
}

fn resources() -> DomainResources {
    let mut read = command("read");
    read.request_contract = Some("demo-request".to_owned());
    let mut root = command("demo");
    root.children.push(read);
    let mut examples = BTreeMap::new();
    examples.insert(
        "demo-request".to_owned(),
        BTreeMap::from([(
            "default".to_owned(),
            Example {
                valid: true,
                example: ContractValue::String("x".to_owned()),
            },
        )]),
    );
    DomainResources {
        domain: "demo".to_owned(),
        root_commands: vec![root],
        contracts: vec![record(
            "demo-request",
            vec![field("schema", true), field("note", false)],
        )],
        examples,
    }
}

fn reason(result: Result<Registry, contract_core::ContractError>) -> &'static str {
    result.expect_err("must be rejected").reason()
}

#[test]
fn a_valid_registry_indexes_commands_contracts_and_examples() {
    let registry = Registry::build(resources()).expect("valid");
    assert_eq!(registry.domain(), "demo");
    assert!(registry.command("demo read").is_some());
    assert_eq!(registry.root_commands().count(), 1);
    assert_eq!(registry.scenarios("demo-request"), ["default"]);
    let listed = query::list(&registry, ListScope::All, None);
    assert_eq!(listed.commands, ["demo", "demo read"]);
    assert_eq!(listed.contracts.len(), 1);
    assert!(
        query::list(&registry, ListScope::Contracts, None)
            .commands
            .is_empty()
    );
    assert!(
        query::list(&registry, ListScope::Commands, None)
            .contracts
            .is_empty()
    );
    assert!(
        query::list(&registry, ListScope::All, Some("zzz"))
            .commands
            .is_empty()
    );
}

#[test]
fn duplicate_command_paths_are_rejected() {
    let mut value = resources();
    value.root_commands.push(command("demo"));
    assert_eq!(reason(Registry::build(value)), "duplicate_command_path");
}

#[test]
fn unknown_command_contract_links_are_rejected() {
    let mut value = resources();
    value.root_commands[0].children[0].request_contract = Some("missing".to_owned());
    assert_eq!(reason(Registry::build(value)), "missing_command_contract");
}

#[test]
fn unknown_field_contract_references_are_rejected() {
    let mut value = resources();
    let target = &mut value.contracts[0].description.fields[1];
    target.field_type = FieldType::Object;
    target.reference = Some("missing".to_owned());
    assert_eq!(reason(Registry::build(value)), "missing_contract_reference");
}

#[test]
fn field_presence_metadata_mismatch_is_rejected() {
    let mut value = resources();
    value.contracts[0].description.fields[0].required = false;
    assert_eq!(reason(Registry::build(value)), "field_presence_mismatch");
    let mut value = resources();
    value.contracts[0]
        .description
        .optional
        .push("schema".to_owned());
    assert_eq!(reason(Registry::build(value)), "field_presence_conflict");
}

#[test]
fn duplicate_fields_and_bad_canonical_order_are_rejected() {
    let mut value = resources();
    value.contracts[0]
        .description
        .fields
        .push(field("note", false));
    assert_eq!(reason(Registry::build(value)), "duplicate_field");
    let mut value = resources();
    value.contracts[0].description.canonical_order.pop();
    assert_eq!(reason(Registry::build(value)), "canonical_order_mismatch");
}

#[test]
fn caller_constructible_is_limited_to_semantic_requests() {
    let mut value = resources();
    value.contracts[0].description.kind = ContractKind::Response;
    assert_eq!(
        reason(Registry::build(value)),
        "caller_constructible_requires_request"
    );
    let mut value = resources();
    value.contracts[0].description.kind = ContractKind::GeneratedRequest;
    assert_eq!(
        reason(Registry::build(value)),
        "caller_constructible_requires_request"
    );
}

#[test]
fn duplicate_flags_and_unknown_group_members_are_rejected() {
    let mut value = resources();
    let node = &mut value.root_commands[0].children[0];
    node.arguments = vec![argument("a", "--same"), argument("b", "--same")];
    assert_eq!(reason(Registry::build(value)), "duplicate_flag");
    let mut value = resources();
    let node = &mut value.root_commands[0].children[0];
    node.arguments = vec![argument("a", "--a")];
    node.exclusive_groups = vec![ExclusiveGroup {
        members: vec!["a".to_owned(), "ghost".to_owned()],
        required: false,
    }];
    assert_eq!(reason(Registry::build(value)), "unknown_group_member");
}

#[test]
fn examples_for_unknown_contracts_are_rejected() {
    let mut value = resources();
    value.examples.insert("ghost".to_owned(), BTreeMap::new());
    assert_eq!(reason(Registry::build(value)), "example_unknown_contract");
}

#[test]
fn queries_report_unknown_targets_with_stable_reasons() {
    let registry = Registry::build(resources()).expect("valid");
    let failure = |error: contract_core::ContractError| error.reason();
    assert_eq!(
        failure(query::describe_contract(&registry, "nope").unwrap_err()),
        "unknown_contract_id"
    );
    assert_eq!(
        failure(query::describe_command(&registry, "nope").unwrap_err()),
        "unknown_command"
    );
    assert_eq!(
        failure(query::example_query(&registry, "nope", "default").unwrap_err()),
        "unknown_contract_id"
    );
    assert_eq!(
        failure(query::example_query(&registry, "demo-request", "nope").unwrap_err()),
        "unknown_scenario"
    );
    assert_eq!(
        failure(query::describe_field(&registry, "demo-request", "/ghost", None).unwrap_err()),
        "unknown_field"
    );
    assert!(query::example_query(&registry, "demo-request", "default").is_ok());
    assert_eq!(
        query::describe_command(&registry, "demo read")
            .unwrap()
            .request_contract
            .as_deref(),
        Some("demo-request")
    );
}

fn branching_record() -> ContractRecord {
    let variant = SchemaNode::Object {
        required: vec!["kind".to_owned(), "value".to_owned()],
        properties: BTreeMap::from([
            (
                "kind".to_owned(),
                SchemaNode::Constant {
                    value: ContractValue::String("a".to_owned()),
                },
            ),
            ("value".to_owned(), SchemaNode::Open),
        ]),
        discriminator: None,
    };
    let selector = SchemaNode::Object {
        required: Vec::new(),
        properties: BTreeMap::new(),
        discriminator: Some(Discriminator {
            property: "kind".to_owned(),
            mapping: BTreeMap::from([("a".to_owned(), "variant-a".to_owned())]),
        }),
    };
    let mut command_field = field("command", true);
    command_field.field_type = FieldType::Object;
    command_field.schema = Some(selector);
    let mut value = record("branching", vec![field("schema", true), command_field]);
    value.description.fields[0].constant = Some(ContractValue::String("branching".to_owned()));
    value.definitions.insert("variant-a".to_owned(), variant);
    value
}

fn branching_registry() -> Registry {
    Registry::build(DomainResources {
        domain: "demo".to_owned(),
        root_commands: Vec::new(),
        contracts: vec![branching_record()],
        examples: BTreeMap::new(),
    })
    .expect("valid")
}

#[test]
fn scaffold_follows_selected_branch_and_reports_unresolved_paths() {
    let registry = branching_registry();
    let data = query::scaffold_query(&registry, "branching", &["/command=a".to_owned()]).unwrap();
    assert_eq!(data.unresolved_paths, ["/command/value"]);
    assert_eq!(data.selected_branches["/command"], "a");
    let ContractValue::Object(object) = data.scaffold else {
        panic!("scaffold must be an object");
    };
    assert_eq!(
        object["schema"],
        ContractValue::String("branching".to_owned())
    );
    let ContractValue::Object(command) = &object["command"] else {
        panic!("command must be an object");
    };
    assert_eq!(command["kind"], ContractValue::String("a".to_owned()));
}

#[test]
fn scaffold_rejects_bad_branch_selections() {
    let registry = branching_registry();
    let reason_of = |branches: &[&str]| {
        let branches: Vec<String> = branches.iter().map(|item| (*item).to_owned()).collect();
        query::scaffold_query(&registry, "branching", &branches)
            .unwrap_err()
            .reason()
    };
    assert_eq!(reason_of(&[]), "missing_branch");
    assert_eq!(reason_of(&["/command=zzz"]), "unknown_variant");
    assert_eq!(reason_of(&["/command"]), "invalid_branch");
    assert_eq!(reason_of(&["/command="]), "invalid_branch");
    assert_eq!(reason_of(&["/command=a", "/command=a"]), "duplicate_branch");
    assert_eq!(reason_of(&["/command=a", "/other=a"]), "invalid_branch");
    assert_eq!(reason_of(&["/bad~2=a"]), "invalid_pointer");
}

#[test]
fn scaffold_refuses_generated_and_non_constructible_contracts() {
    let mut generated = record("generated", vec![field("schema", true)]);
    generated.description.kind = ContractKind::GeneratedRequest;
    generated.description.caller_constructible = false;
    let mut response = record("response", vec![field("schema", true)]);
    response.description.kind = ContractKind::Response;
    response.description.caller_constructible = false;
    let registry = Registry::build(DomainResources {
        domain: "demo".to_owned(),
        root_commands: Vec::new(),
        contracts: vec![generated, response],
        examples: BTreeMap::new(),
    })
    .expect("valid");
    assert_eq!(
        query::scaffold_query(&registry, "generated", &[])
            .unwrap_err()
            .reason(),
        "generated_request_not_caller_constructible"
    );
    assert_eq!(
        query::scaffold_query(&registry, "response", &[])
            .unwrap_err()
            .reason(),
        "contract_scaffold_requires_request"
    );
}

#[test]
fn field_description_exposes_variants_and_rejects_bad_selection() {
    let registry = branching_registry();
    let data = query::describe_field(&registry, "branching", "/command", None).unwrap();
    assert_eq!(data.discriminator.as_deref(), Some("kind"));
    assert_eq!(data.variants, ["a"]);
    let chosen = query::describe_field(&registry, "branching", "/command", Some("a")).unwrap();
    assert_eq!(chosen.variant.as_deref(), Some("a"));
    assert_eq!(
        query::describe_field(&registry, "branching", "/command", Some("zzz"))
            .unwrap_err()
            .reason(),
        "unknown_variant"
    );
    assert_eq!(
        query::describe_field(&registry, "branching", "/schema", Some("a"))
            .unwrap_err()
            .reason(),
        "variant_not_applicable"
    );
}

#[test]
fn clap_tree_matches_the_declared_command_tree() {
    let registry = Registry::build(resources()).expect("valid");
    let root = &registry.command("demo").expect("root").node;
    let tree = contract_core::clap_tree::build(root);
    tree.clone().debug_assert();
    assert!(tree.clone().try_get_matches_from(["demo", "read"]).is_ok());
    assert!(
        tree.clone()
            .try_get_matches_from(["demo", "ghost"])
            .is_err()
    );
    assert!(tree.try_get_matches_from(["demo"]).is_err());
}
