//! Public contract descriptions of the Skill CLI: schema identity and field
//! descriptions. The command tree lives in `skill-cli`.

use std::collections::BTreeMap;

use contract_core::model::{
    ContractDescription, ContractKind, ContractRecord, ContractValue, FieldDescription, FieldType,
    SchemaNode,
};

/// Domain name served by the Skill CLI.
pub const DOMAIN: &str = "skill";

pub const RESOLVE_REQUEST: &str = "skill-resolve-request";
pub const DISCOVER_RESPONSE: &str = "skill-discover-response";
pub const RESOLVE_RESPONSE: &str = "skill-resolve-response";
pub const SWITCH_RESPONSE: &str = "skill-switch-response";
pub const VALIDATE_RESPONSE: &str = "skill-validate-response";

const SELECTION: &str = "selection";

fn field(name: &str, field_type: FieldType, required: bool) -> FieldDescription {
    FieldDescription {
        name: name.to_owned(),
        field_type,
        nullable: false,
        required,
        reference: None,
        constant: None,
        schema: None,
    }
}

fn record(id: &str, kind: ContractKind, mut fields: Vec<FieldDescription>) -> ContractRecord {
    fields.insert(0, {
        let mut schema = field("schema", FieldType::String, true);
        schema.constant = Some(ContractValue::String(id.to_owned()));
        schema
    });
    let names = |keep: bool| {
        fields
            .iter()
            .filter(|item| item.required == keep)
            .map(|item| item.name.clone())
            .collect::<Vec<_>>()
    };
    ContractRecord {
        description: ContractDescription {
            id: id.to_owned(),
            kind,
            caller_constructible: kind == ContractKind::SemanticRequest,
            required: names(true),
            optional: names(false),
            canonical_order: fields.iter().map(|item| item.name.clone()).collect(),
            fields,
            example: None,
        },
        definitions: BTreeMap::new(),
    }
}

fn resolve_request() -> ContractRecord {
    let mut selections = field("selections", FieldType::Array, true);
    selections.schema = Some(SchemaNode::Array {
        items: Box::new(SchemaNode::Reference {
            reference: SELECTION.to_owned(),
        }),
        min_items: 1,
    });
    let mut value = record(
        RESOLVE_REQUEST,
        ContractKind::SemanticRequest,
        vec![selections, field("modules", FieldType::Array, false)],
    );
    value.definitions.insert(
        SELECTION.to_owned(),
        SchemaNode::Object {
            required: vec!["skill".to_owned()],
            properties: BTreeMap::from([
                ("skill".to_owned(), SchemaNode::Open),
                ("nodes".to_owned(), SchemaNode::Open),
                ("references".to_owned(), SchemaNode::Open),
            ]),
            discriminator: None,
        },
    );
    value
}

fn response(id: &str, fields: &[(&str, FieldType)]) -> ContractRecord {
    record(
        id,
        ContractKind::Response,
        fields
            .iter()
            .map(|(name, field_type)| field(name, *field_type, true))
            .collect(),
    )
}

/// Every public contract of the Skill CLI.
pub fn contract_records() -> Vec<ContractRecord> {
    use FieldType::{Array, Boolean, String};
    vec![
        resolve_request(),
        response(
            DISCOVER_RESPONSE,
            &[
                ("skill", String),
                ("node", String),
                ("children", Array),
                ("failures", Array),
            ],
        ),
        response(RESOLVE_RESPONSE, &[("sources", Array)]),
        response(
            SWITCH_RESPONSE,
            &[("reused", Array), ("added", Array), ("dropped", Array)],
        ),
        response(
            VALIDATE_RESPONSE,
            &[("valid", Boolean), ("checked", Array), ("failures", Array)],
        ),
    ]
}
