//! Public contract descriptions of the Work CLI: schema identity, field
//! descriptions and record kinds. Command trees live in `work-cli`.

use std::collections::BTreeMap;

use contract_core::model::{
    ContractDescription, ContractKind, ContractRecord, ContractValue, FieldDescription, FieldType,
    SchemaNode,
};

use crate::{
    APPLY_REQUEST_SCHEMA, CORRECTION_LIST_RESPONSE, CORRECTION_PREVIEW_RESPONSE,
    DISCUSSION_CREATE_SCHEMA, DISCUSSION_RESPONSE, DISCUSSION_ROUND_SCHEMA, DRAFT_REQUEST_SCHEMA,
    EXECUTION_RECORD_SCHEMA, EXECUTION_RESPONSE, HISTORY_RESPONSE, PROGRESS_RESPONSE,
    REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA, REQUIREMENT_ACCEPTANCE_RESPONSE, SOURCE_CREATE_SCHEMA,
    SOURCE_RESPONSE, STATUS_RESPONSE, TASK_ACCEPTANCE_RECORD_SCHEMA, TASK_ACCEPTANCE_RESPONSE,
    TASK_APPLY_SCHEMA, TASK_COMPLETE_SCHEMA, TASK_CREATE_SCHEMA, TASK_PREVIEW_RESPONSE,
    TASK_PREVIEW_SCHEMA, TASK_RESPONSE, TASK_REVISE_SCHEMA, TRACE_RESPONSE, WRITE_RESPONSE,
};

/// Domain name served by the Work CLI.
pub const DOMAIN: &str = "work";

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

fn request(id: &str, fields: &[(&str, FieldType, bool)]) -> ContractRecord {
    record(
        id,
        ContractKind::SemanticRequest,
        fields
            .iter()
            .map(|(name, kind, required)| field(name, *kind, *required))
            .collect(),
    )
}

fn response(id: &str, fields: &[(&str, FieldType)]) -> ContractRecord {
    record(
        id,
        ContractKind::Response,
        fields
            .iter()
            .map(|(name, kind)| field(name, *kind, true))
            .collect(),
    )
}

/// Every public contract of the Work CLI.
pub fn contract_records() -> Vec<ContractRecord> {
    use FieldType::{Array, Boolean, Integer, Object, String};
    let mut source = request(
        SOURCE_CREATE_SCHEMA,
        &[
            ("id", String, true),
            ("title", String, true),
            ("locator", String, true),
            // The CLI can supply content using --from-file instead.
            ("content", Object, false),
            ("origin", Object, true),
        ],
    );
    let content = source
        .description
        .fields
        .iter_mut()
        .find(|field| field.name == "content")
        .expect("source content field");
    content.schema = Some(SchemaNode::Branch {
        alternatives: vec![
            SchemaNode::Object {
                required: vec!["kind".to_owned(), "text".to_owned()],
                properties: BTreeMap::from([
                    (
                        "kind".to_owned(),
                        SchemaNode::Constant {
                            value: ContractValue::String("text".to_owned()),
                        },
                    ),
                    ("text".to_owned(), SchemaNode::Open),
                ]),
                discriminator: None,
            },
            SchemaNode::Object {
                required: vec!["kind".to_owned(), "bytes".to_owned()],
                properties: BTreeMap::from([
                    (
                        "kind".to_owned(),
                        SchemaNode::Constant {
                            value: ContractValue::String("binary".to_owned()),
                        },
                    ),
                    (
                        "bytes".to_owned(),
                        SchemaNode::Array {
                            items: Box::new(SchemaNode::Open),
                            min_items: 0,
                        },
                    ),
                ]),
                discriminator: None,
            },
        ],
    });
    vec![
        source,
        request(
            DISCUSSION_CREATE_SCHEMA,
            &[
                ("id", String, true),
                ("original_request", String, true),
                ("source_ids", Array, false),
            ],
        ),
        request(
            DISCUSSION_ROUND_SCHEMA,
            &[
                ("id", String, true),
                ("expected_version", Integer, true),
                ("answer", Object, false),
                ("decisions", Array, false),
                ("add_pending", Array, false),
                ("resolve_pending", Array, false),
                ("link_sources", Array, false),
                ("ask", Object, false),
                ("close", Boolean, false),
            ],
        ),
        request(
            DRAFT_REQUEST_SCHEMA,
            &[
                ("id", String, true),
                ("target", Object, true),
                ("statement", String, true),
                ("reason", String, true),
                ("origin", Object, true),
            ],
        ),
        request(
            APPLY_REQUEST_SCHEMA,
            &[
                ("draft", Object, true),
                ("approved_candidate", String, true),
                ("reviewed_target_version", Integer, true),
            ],
        ),
        response(
            WRITE_RESPONSE,
            &[("kind", String), ("id", String), ("version", Integer)],
        ),
        response(
            SOURCE_RESPONSE,
            &[
                ("record", Object),
                ("referenced_by", Array),
                ("corrections", Array),
            ],
        ),
        response(
            DISCUSSION_RESPONSE,
            &[
                ("record", Object),
                ("latest_version", Integer),
                ("corrections", Array),
            ],
        ),
        response(HISTORY_RESPONSE, &[("id", String), ("versions", Array)]),
        response(
            PROGRESS_RESPONSE,
            &[
                ("id", String),
                ("version", Integer),
                ("status", String),
                ("questions", Integer),
                ("answered", Integer),
                ("outstanding_question", Object),
                ("open_pending", Array),
                ("decisions", Array),
            ],
        ),
        response(STATUS_RESPONSE, &[("records", Array)]),
        response(
            CORRECTION_PREVIEW_RESPONSE,
            &[("candidate", String), ("reviewed_target_version", Integer)],
        ),
        response(CORRECTION_LIST_RESPONSE, &[("corrections", Array)]),
        request(
            TASK_CREATE_SCHEMA,
            &[
                ("id", String, true),
                ("title", String, true),
                ("goal", String, true),
                ("requirements", Array, true),
                ("constraints", Array, false),
                ("decisions", Array, false),
                ("criteria", Array, true),
                ("discussion_ids", Array, false),
                ("source_ids", Array, false),
                ("notes", Array, false),
            ],
        ),
        request(
            TASK_REVISE_SCHEMA,
            &[
                ("id", String, true),
                ("expected_version", Integer, true),
                ("reason", String, true),
                ("title", String, false),
                ("goal", String, false),
                ("requirements", Array, false),
                ("constraints", Array, false),
                ("decisions", Array, false),
                ("criteria", Array, false),
                ("notes", Array, false),
                ("link_discussions", Array, false),
                ("link_sources", Array, false),
                ("reconfirm_criteria", Array, false),
                ("reconfirm_requirement", Boolean, false),
            ],
        ),
        request(TASK_PREVIEW_SCHEMA, &[("draft", Object, true)]),
        request(
            TASK_APPLY_SCHEMA,
            &[
                ("draft", Object, true),
                ("approved_candidate", String, true),
                ("reviewed_dependencies", Array, true),
            ],
        ),
        response(
            TASK_PREVIEW_RESPONSE,
            &[("candidate", String), ("reviewed_dependencies", Array)],
        ),
        request(
            TASK_COMPLETE_SCHEMA,
            &[("id", String, true), ("expected_version", Integer, true)],
        ),
        request(
            EXECUTION_RECORD_SCHEMA,
            &[
                ("id", String, true),
                ("task_id", String, true),
                ("expected_version", Integer, false),
                ("attempt", Object, true),
            ],
        ),
        request(
            TASK_ACCEPTANCE_RECORD_SCHEMA,
            &[
                ("id", String, true),
                ("task_id", String, true),
                ("task_version", Integer, true),
                ("expected_version", Integer, false),
                ("conclusions", Array, true),
            ],
        ),
        request(
            REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA,
            &[
                ("id", String, true),
                ("discussion_id", String, true),
                ("expected_version", Integer, false),
                ("verdict", String, true),
                ("evidence", String, true),
                ("origin", Object, true),
                ("task_refs", Array, false),
            ],
        ),
        response(
            TASK_RESPONSE,
            &[
                ("record", Object),
                ("latest_version", Integer),
                ("criteria", Array),
                ("completable", Boolean),
                ("executions", Array),
                ("acceptance", Object),
                ("corrections", Array),
            ],
        ),
        response(
            EXECUTION_RESPONSE,
            &[
                ("record", Object),
                ("latest_version", Integer),
                ("corrections", Array),
            ],
        ),
        response(
            TASK_ACCEPTANCE_RESPONSE,
            &[
                ("record", Object),
                ("latest_version", Integer),
                ("criteria", Array),
                ("corrections", Array),
            ],
        ),
        response(
            REQUIREMENT_ACCEPTANCE_RESPONSE,
            &[
                ("record", Object),
                ("latest_version", Integer),
                ("state", Object),
                ("corrections", Array),
            ],
        ),
        response(
            TRACE_RESPONSE,
            &[
                ("root", Object),
                ("sources", Array),
                ("discussions", Array),
                ("tasks", Array),
                ("requirement_acceptances", Array),
            ],
        ),
    ]
}
