//! Structural scaffold derivation from a contract's declared schema.
//!
//! Scaffolds describe shape only. They never stand in for business validation,
//! evidence or approval.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::{ContractError, ContractResult};
use crate::model::{ContractRecord, ContractValue, SchemaNode};

/// Result of deriving a scaffold: value, unresolved pointers and used branches.
#[derive(Debug, Clone, PartialEq)]
pub struct Scaffold {
    pub value: ContractValue,
    pub unresolved_paths: Vec<String>,
    pub selected_branches: BTreeMap<String, String>,
}

#[derive(Default)]
struct Builder {
    unresolved: BTreeSet<String>,
    used: BTreeSet<String>,
}

/// Derive the scaffold of a caller-constructible request contract.
pub fn derive(
    record: &ContractRecord,
    branches: BTreeMap<String, String>,
) -> ContractResult<Scaffold> {
    let mut builder = Builder::default();
    let mut object = BTreeMap::new();
    for field in &record.description.fields {
        if !field.required {
            continue;
        }
        let pointer = format!("/{}", encode_segment(&field.name));
        let value = if let Some(constant) = &field.constant {
            constant.clone()
        } else if let Some(schema) = &field.schema {
            builder.node(schema, record, &pointer, &branches, 0)?
        } else if field.field_type == crate::model::FieldType::Array {
            ContractValue::Array(Vec::new())
        } else {
            builder.unresolved.insert(pointer.clone());
            ContractValue::Null
        };
        object.insert(field.name.clone(), value);
    }
    for pointer in branches.keys() {
        if !builder.used.contains(pointer) {
            return Err(ContractError::new(
                "invalid_branch",
                format!("Branch is unknown or its parent was not selected: {pointer}"),
            ));
        }
    }
    Ok(Scaffold {
        value: ContractValue::Object(object),
        unresolved_paths: builder.unresolved.into_iter().collect(),
        selected_branches: branches,
    })
}

impl Builder {
    fn node(
        &mut self,
        schema: &SchemaNode,
        record: &ContractRecord,
        pointer: &str,
        branches: &BTreeMap<String, String>,
        depth: usize,
    ) -> ContractResult<ContractValue> {
        if depth > 32 {
            return Err(ContractError::new(
                "schema_too_deep",
                "Schema nesting is too deep.",
            ));
        }
        match schema {
            SchemaNode::Reference { reference } => {
                let target = record.definitions.get(reference).ok_or_else(|| {
                    ContractError::new(
                        "missing_schema_definition",
                        format!("Unknown definition: {reference}"),
                    )
                })?;
                self.node(target, record, pointer, branches, depth + 1)
            }
            SchemaNode::Constant { value } => Ok(value.clone()),
            SchemaNode::Open => {
                self.unresolved.insert(pointer.to_owned());
                Ok(ContractValue::Null)
            }
            SchemaNode::Array { min_items, .. } if *min_items == 0 => {
                Ok(ContractValue::Array(Vec::new()))
            }
            SchemaNode::Array { .. } => {
                self.unresolved.insert(pointer.to_owned());
                Ok(ContractValue::Null)
            }
            SchemaNode::Branch { alternatives } => {
                self.node(&alternatives[0], record, pointer, branches, depth + 1)
            }
            SchemaNode::Object {
                required,
                properties,
                discriminator,
            } => {
                if let Some(discriminator) = discriminator {
                    let variant = branches.get(pointer).ok_or_else(|| {
                        ContractError::new(
                            "missing_branch",
                            format!("Branch must be selected: {pointer}"),
                        )
                    })?;
                    let reference = discriminator.mapping.get(variant).ok_or_else(|| {
                        ContractError::new(
                            "unknown_variant",
                            format!("Unknown variant at {pointer}: {variant}"),
                        )
                    })?;
                    self.used.insert(pointer.to_owned());
                    let target = record.definitions.get(reference).ok_or_else(|| {
                        ContractError::new(
                            "missing_schema_definition",
                            format!("Unknown definition: {reference}"),
                        )
                    })?;
                    return self.node(target, record, pointer, branches, depth + 1);
                }
                let mut object = BTreeMap::new();
                for name in required {
                    let child = properties.get(name).ok_or_else(|| {
                        ContractError::new(
                            "missing_schema_property",
                            format!("Missing required property: {name}"),
                        )
                    })?;
                    let path = format!("{pointer}/{}", encode_segment(name));
                    object.insert(
                        name.clone(),
                        self.node(child, record, &path, branches, depth + 1)?,
                    );
                }
                Ok(ContractValue::Object(object))
            }
        }
    }
}

/// Encode a segment as an RFC 6901 JSON pointer token.
pub fn encode_segment(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

/// Decode a field pointer into its tokens.
pub fn decode_pointer(pointer: &str) -> ContractResult<Vec<String>> {
    if !pointer.starts_with('/') || pointer.len() == 1 {
        return Err(ContractError::new(
            "invalid_pointer",
            "A field pointer must start with / and name a field.",
        ));
    }
    pointer[1..]
        .split('/')
        .map(|segment| {
            let mut decoded = String::new();
            let mut chars = segment.chars();
            while let Some(character) = chars.next() {
                if character == '~' {
                    match chars.next() {
                        Some('0') => decoded.push('~'),
                        Some('1') => decoded.push('/'),
                        _ => {
                            return Err(ContractError::new(
                                "invalid_pointer",
                                "Invalid RFC 6901 escape.",
                            ));
                        }
                    }
                } else {
                    decoded.push(character);
                }
            }
            Ok(decoded)
        })
        .collect()
}

/// Resolve `pointer` against a schema node inside the record's definitions.
pub fn resolve<'a>(schema: &'a SchemaNode, tokens: &[String]) -> Option<&'a SchemaNode> {
    let mut current = schema;
    for token in tokens {
        match current {
            SchemaNode::Object { properties, .. } => current = properties.get(token)?,
            _ => return None,
        }
    }
    Some(current)
}

#[cfg(test)]
mod tests;
