//! Structural validation of a domain registry.

use std::collections::BTreeSet;

use crate::error::{ContractError, ContractResult};
use crate::model::{ArgumentAction, ContractKind, ContractRecord, FieldType, SchemaNode};
use crate::registry::Registry;

fn fail(reason: &'static str, message: impl Into<String>) -> ContractError {
    ContractError::new(reason, message)
}

/// Validate one contract record in isolation from its siblings.
///
/// Cross-record checks (references, command links) live in [`validate`].
pub fn validate_contract(record: &ContractRecord) -> ContractResult<()> {
    let description = &record.description;
    let id = &description.id;
    if id.is_empty() {
        return Err(fail("empty_contract_id", "Contract ID must not be empty."));
    }
    if description.caller_constructible && description.kind != ContractKind::SemanticRequest {
        return Err(fail(
            "caller_constructible_requires_request",
            format!("Only semantic requests are caller constructible: {id}"),
        ));
    }
    if description.kind == ContractKind::GeneratedRequest && description.caller_constructible {
        return Err(fail(
            "generated_request_not_caller_constructible",
            format!("Generated request must not be caller constructible: {id}"),
        ));
    }

    let required: BTreeSet<&str> = description.required.iter().map(String::as_str).collect();
    let optional: BTreeSet<&str> = description.optional.iter().map(String::as_str).collect();
    if !required.is_disjoint(&optional) {
        return Err(fail(
            "field_presence_conflict",
            format!("Required and optional fields overlap: {id}"),
        ));
    }

    let mut names = BTreeSet::new();
    for field in &description.fields {
        if field.name.is_empty() {
            return Err(fail("empty_field_name", format!("Empty field name: {id}")));
        }
        if !names.insert(field.name.as_str()) {
            return Err(fail(
                "duplicate_field",
                format!("Duplicate field: {id}/{}", field.name),
            ));
        }
        if required.contains(field.name.as_str()) == optional.contains(field.name.as_str()) {
            return Err(fail(
                "field_presence_mismatch",
                format!(
                    "Field must be exactly required or optional: {id}/{}",
                    field.name
                ),
            ));
        }
        if field.required != required.contains(field.name.as_str()) {
            return Err(fail(
                "field_presence_mismatch",
                format!(
                    "Field presence flag disagrees with list: {id}/{}",
                    field.name
                ),
            ));
        }
        if field.constant.is_some() && field.schema.is_some() {
            return Err(fail(
                "ambiguous_field_constraint",
                format!(
                    "Field declares both constant and schema: {id}/{}",
                    field.name
                ),
            ));
        }
        if let Some(schema) = &field.schema {
            validate_schema(schema, record, &field.name)?;
        }
    }

    let declared: BTreeSet<&str> = required.union(&optional).copied().collect();
    if names != declared {
        return Err(fail(
            "field_presence_mismatch",
            format!("Field list disagrees with required/optional: {id}"),
        ));
    }

    let order: BTreeSet<&str> = description
        .canonical_order
        .iter()
        .map(String::as_str)
        .collect();
    if description.canonical_order.len() != names.len() || order != names {
        return Err(fail(
            "canonical_order_mismatch",
            format!("Canonical order must list every field exactly once: {id}"),
        ));
    }
    Ok(())
}

fn validate_schema(
    schema: &SchemaNode,
    record: &ContractRecord,
    field: &str,
) -> ContractResult<()> {
    match schema {
        SchemaNode::Reference { reference } => {
            if !record.definitions.contains_key(reference) {
                return Err(fail(
                    "missing_schema_definition",
                    format!(
                        "Unknown definition {reference} for {}/{}",
                        record.description.id, field
                    ),
                ));
            }
        }
        SchemaNode::Constant { .. } | SchemaNode::Open => {}
        SchemaNode::Object {
            required,
            properties,
            discriminator,
        } => {
            for name in required {
                if !properties.contains_key(name) {
                    return Err(fail(
                        "missing_schema_property",
                        format!(
                            "Required property {name} is not declared for {}/{}",
                            record.description.id, field
                        ),
                    ));
                }
            }
            for property in properties.values() {
                validate_schema(property, record, field)?;
            }
            if let Some(discriminator) = discriminator {
                for reference in discriminator.mapping.values() {
                    if !record
                        .definitions
                        .contains_key(reference.trim_start_matches("#/"))
                    {
                        return Err(fail(
                            "missing_schema_definition",
                            format!(
                                "Unknown discriminator target {reference} for {}/{}",
                                record.description.id, field
                            ),
                        ));
                    }
                }
            }
        }
        SchemaNode::Array { items, .. } => validate_schema(items, record, field)?,
        SchemaNode::Branch { alternatives } => {
            if alternatives.is_empty() {
                return Err(fail(
                    "empty_branch",
                    format!(
                        "Branch has no alternatives for {}/{}",
                        record.description.id, field
                    ),
                ));
            }
            for alternative in alternatives {
                validate_schema(alternative, record, field)?;
            }
        }
    }
    Ok(())
}

fn validate_command(registry: &Registry, path: &str) -> ContractResult<()> {
    let entry = registry
        .command(path)
        .ok_or_else(|| fail("unknown_command", format!("Unknown command: {path}")))?;
    let node = &entry.node;
    if node.name.is_empty() || node.name.contains(char::is_whitespace) {
        return Err(fail(
            "invalid_command_name",
            format!("Invalid command name at {path}"),
        ));
    }
    let mut names = BTreeSet::new();
    let mut flags = BTreeSet::new();
    for argument in &node.arguments {
        if !names.insert(argument.name.as_str()) {
            return Err(fail(
                "duplicate_argument",
                format!("Duplicate argument {path}: {}", argument.name),
            ));
        }
        if argument.action == ArgumentAction::Set && argument.flags.is_empty() {
            return Err(fail(
                "missing_argument_flag",
                format!("Value argument has no flag {path}: {}", argument.name),
            ));
        }
        for flag in &argument.flags {
            if !flags.insert(flag.as_str()) {
                return Err(fail(
                    "duplicate_flag",
                    format!("Duplicate flag {path}: {flag}"),
                ));
            }
        }
        if argument.action == ArgumentAction::SetTrue && argument.nargs_any {
            return Err(fail(
                "invalid_argument_action",
                format!("Boolean flag cannot accept any number of values: {path}"),
            ));
        }
        if argument.nargs_any && argument.action != ArgumentAction::Append {
            return Err(fail(
                "invalid_argument_action",
                format!("Only append arguments accept any number of values: {path}"),
            ));
        }
    }
    for group in &node.exclusive_groups {
        if group.members.len() < 2 {
            return Err(fail(
                "invalid_exclusive_group",
                format!("Exclusive group needs at least two members: {path}"),
            ));
        }
        for member in &group.members {
            if !names.contains(member.as_str()) {
                return Err(fail(
                    "unknown_group_member",
                    format!("Unknown exclusive group member {path}: {member}"),
                ));
            }
        }
    }
    for link in [&node.request_contract, &node.response_contract] {
        if let Some(id) = link
            && registry.contract(id).is_none()
        {
            return Err(fail(
                "missing_command_contract",
                format!("Command links unknown contract {path}: {id}"),
            ));
        }
    }
    Ok(())
}

/// Validate the whole registry: every contract, every command and every link.
pub fn validate(registry: &Registry) -> ContractResult<()> {
    let mut ids = BTreeSet::new();
    for contract in registry.contracts() {
        validate_contract(contract)?;
        ids.insert(contract.description.id.as_str());
    }
    for contract in registry.contracts() {
        let description = &contract.description;
        for field in &description.fields {
            if let Some(reference) = &field.reference
                && !ids.contains(reference.as_str())
            {
                return Err(fail(
                    "missing_contract_reference",
                    format!("Unknown contract reference {}: {reference}", description.id),
                ));
            }
            if field.reference.is_some() && field.field_type != FieldType::Object {
                return Err(fail(
                    "invalid_contract_reference",
                    format!(
                        "Only object fields may reference another contract: {}/{}",
                        description.id, field.name
                    ),
                ));
            }
        }
    }
    let paths: Vec<String> = registry
        .commands()
        .map(|entry| entry.path.clone())
        .collect();
    for path in paths {
        validate_command(registry, &path)?;
    }
    for (id, scenarios) in registry.examples_index() {
        if !ids.contains(id.as_str()) {
            return Err(fail(
                "example_unknown_contract",
                format!("Example references unknown contract: {id}"),
            ));
        }
        for scenario in scenarios.keys() {
            if scenario.is_empty() {
                return Err(fail("empty_scenario", format!("Empty scenario: {id}")));
            }
        }
    }
    Ok(())
}
