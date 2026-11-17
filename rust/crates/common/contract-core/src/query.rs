//! On-demand public contract queries. Business validation stays in each domain.

use std::collections::BTreeMap;

use crate::error::{ContractError, ContractResult};
use crate::model::{ArgumentAction, ContractKind, ContractRecord};
use crate::payload::{
    ArgumentDescription, ArgumentValueType, CommandDescriptionData, ContractDescriptionData,
    ContractIndex, ExampleData, FieldDescriptionData, FieldSummary, ListData, ScaffoldData,
};
use crate::registry::Registry;
use crate::scaffold;

/// Query scope of a list request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListScope {
    All,
    Commands,
    Contracts,
}

pub fn list(registry: &Registry, scope: ListScope, prefix: Option<&str>) -> ListData {
    let prefix = prefix.unwrap_or("");
    let commands = if scope == ListScope::Contracts {
        Vec::new()
    } else {
        registry
            .commands()
            .map(|entry| entry.path.as_str())
            .filter(|path| path.starts_with(prefix))
            .map(str::to_owned)
            .collect()
    };
    let contracts = if scope == ListScope::Commands {
        Vec::new()
    } else {
        registry
            .contracts()
            .filter(|record| record.description.id.starts_with(prefix))
            .map(|record| ContractIndex {
                contract_id: record.description.id.clone(),
                kind: record.description.kind,
                caller_constructible: record.description.caller_constructible,
            })
            .collect()
    };
    ListData {
        commands,
        contracts,
    }
}

pub fn describe_command(registry: &Registry, path: &str) -> ContractResult<CommandDescriptionData> {
    let entry = registry
        .command(path)
        .ok_or_else(|| ContractError::new("unknown_command", format!("Unknown command: {path}")))?;
    let node = &entry.node;
    let arguments = node
        .arguments
        .iter()
        .map(|argument| {
            let exclusive_with = node
                .exclusive_groups
                .iter()
                .filter(|group| group.members.iter().any(|member| member == &argument.name))
                .flat_map(|group| group.members.iter())
                .filter(|member| **member != argument.name)
                .cloned()
                .collect();
            ArgumentDescription {
                name: argument.name.clone(),
                flags: argument.flags.clone(),
                required: argument.required,
                value_type: match argument.action {
                    ArgumentAction::SetTrue => ArgumentValueType::Boolean,
                    ArgumentAction::Append => ArgumentValueType::Array,
                    ArgumentAction::Set if argument.nargs_any => ArgumentValueType::Array,
                    ArgumentAction::Set => ArgumentValueType::String,
                },
                choices: argument.choices.clone(),
                exclusive_with,
                help: argument.help.clone(),
            }
        })
        .collect();
    Ok(CommandDescriptionData {
        command: path.to_owned(),
        arguments,
        request_contract: node.request_contract.clone(),
        response_contract: node.response_contract.clone(),
    })
}

pub fn describe_contract(registry: &Registry, id: &str) -> ContractResult<ContractDescriptionData> {
    let record = registry.contract(id).ok_or_else(|| {
        ContractError::new("unknown_contract_id", format!("Unknown contract ID: {id}"))
    })?;
    let fields = record
        .description
        .fields
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                FieldSummary {
                    name: field.name.clone(),
                    field_type: field.field_type,
                    nullable: field.nullable,
                    required: field.required,
                    reference: field.reference.clone(),
                    details_available: field.schema.is_some() || field.constant.is_some(),
                },
            )
        })
        .collect();
    Ok(ContractDescriptionData {
        contract_id: id.to_owned(),
        kind: record.description.kind,
        fields,
        scenarios: registry
            .scenarios(id)
            .into_iter()
            .map(str::to_owned)
            .collect(),
    })
}

pub fn describe_field(
    registry: &Registry,
    id: &str,
    pointer: &str,
    variant: Option<&str>,
) -> ContractResult<FieldDescriptionData> {
    let record = registry.contract(id).ok_or_else(|| {
        ContractError::new("unknown_contract_id", format!("Unknown contract ID: {id}"))
    })?;
    let tokens = scaffold::decode_pointer(pointer)?;
    let Some((first, rest)) = tokens.split_first() else {
        return Err(ContractError::new(
            "invalid_pointer",
            "A field pointer must name a field.",
        ));
    };
    let field = record
        .description
        .fields
        .iter()
        .find(|field| field.name == *first)
        .ok_or_else(|| ContractError::new("unknown_field", format!("Unknown field: {pointer}")))?;
    let mut data = FieldDescriptionData {
        contract_id: id.to_owned(),
        path: pointer.to_owned(),
        name: tokens.last().cloned().unwrap_or_default(),
        field_type: None,
        required: None,
        nullable: None,
        reference: None,
        discriminator: None,
        variants: Vec::new(),
        variant: None,
        constant: None,
    };
    if rest.is_empty() {
        data.field_type = Some(field.field_type);
        data.required = Some(field.required);
        data.nullable = Some(field.nullable);
        data.reference = field.reference.clone();
        data.constant = field.constant.clone();
    }
    match &field.schema {
        Some(schema) => {
            let node = scaffold::resolve(schema, rest).ok_or_else(|| {
                ContractError::new("unknown_field", format!("Unknown field: {pointer}"))
            })?;
            if let crate::model::SchemaNode::Object {
                discriminator: Some(discriminator),
                ..
            } = node
            {
                data.discriminator = Some(discriminator.property.clone());
                data.variants = discriminator.mapping.keys().cloned().collect();
                let Some(selected) = variant else {
                    return Ok(data);
                };
                if !discriminator.mapping.contains_key(selected) {
                    return Err(ContractError::new(
                        "unknown_variant",
                        format!("Unknown variant: {selected}"),
                    ));
                }
                data.variant = Some(selected.to_owned());
            } else if variant.is_some() {
                return Err(ContractError::new(
                    "variant_not_applicable",
                    "The selected field has no discriminator.",
                ));
            }
        }
        None => {
            if !rest.is_empty() {
                return Err(ContractError::new(
                    "unknown_field",
                    format!("Unknown field: {pointer}"),
                ));
            }
            if variant.is_some() {
                return Err(ContractError::new(
                    "variant_not_applicable",
                    "The selected field has no discriminator.",
                ));
            }
        }
    }
    Ok(data)
}

pub fn example_query(registry: &Registry, id: &str, scenario: &str) -> ContractResult<ExampleData> {
    let Some(example) = registry
        .contract(id)
        .and_then(|_| registry.example(id, scenario))
    else {
        return match registry.contract(id) {
            None => Err(ContractError::new(
                "unknown_contract_id",
                format!("Unknown contract ID: {id}"),
            )),
            Some(_) => Err(ContractError::new(
                "unknown_scenario",
                format!("Unknown scenario: {scenario}"),
            )),
        };
    };
    Ok(ExampleData {
        contract_id: id.to_owned(),
        scenario: scenario.to_owned(),
        valid: example.valid,
        example: example.example.clone(),
    })
}

pub fn scaffold_query(
    registry: &Registry,
    id: &str,
    branch_args: &[String],
) -> ContractResult<ScaffoldData> {
    let record = request_record(registry, id)?;
    let mut branches = BTreeMap::new();
    for raw in branch_args {
        let (pointer, variant) = raw.split_once('=').ok_or_else(|| {
            ContractError::new("invalid_branch", "Branch must be <pointer>=<variant>.")
        })?;
        scaffold::decode_pointer(pointer)?;
        if variant.is_empty() {
            return Err(ContractError::new(
                "invalid_branch",
                "Branch variant is empty.",
            ));
        }
        if branches
            .insert(pointer.to_owned(), variant.to_owned())
            .is_some()
        {
            return Err(ContractError::new(
                "duplicate_branch",
                format!("Duplicate branch: {pointer}"),
            ));
        }
    }
    let scaffold = scaffold::derive(record, branches)?;
    Ok(ScaffoldData {
        contract_id: id.to_owned(),
        scaffold: scaffold.value,
        unresolved_paths: scaffold.unresolved_paths,
        selected_branches: scaffold.selected_branches,
    })
}

fn request_record<'a>(registry: &'a Registry, id: &str) -> ContractResult<&'a ContractRecord> {
    let record = registry.contract(id).ok_or_else(|| {
        ContractError::new("unknown_contract_id", format!("Unknown contract ID: {id}"))
    })?;
    if record.description.kind == ContractKind::GeneratedRequest {
        return Err(ContractError::new(
            "generated_request_not_caller_constructible",
            "Generated requests must come from their formal prepare flow.",
        ));
    }
    if !record.description.caller_constructible {
        return Err(ContractError::new(
            "contract_scaffold_requires_request",
            "Only caller-constructible requests can be scaffolded.",
        ));
    }
    Ok(record)
}
