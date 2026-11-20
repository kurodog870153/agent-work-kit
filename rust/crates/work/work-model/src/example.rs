//! Declared examples of the Work CLI request contracts.

use std::collections::BTreeMap;

use contract_core::model::{ContractValue, Example};

use crate::{
    APPLY_REQUEST_SCHEMA, DISCUSSION_CREATE_SCHEMA, DISCUSSION_ROUND_SCHEMA, DRAFT_REQUEST_SCHEMA,
    EXECUTION_RECORD_SCHEMA, REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA, SOURCE_CREATE_SCHEMA,
    TASK_ACCEPTANCE_RECORD_SCHEMA, TASK_APPLY_SCHEMA, TASK_COMPLETE_SCHEMA, TASK_CREATE_SCHEMA,
    TASK_PREVIEW_SCHEMA, TASK_REVISE_SCHEMA,
};

fn text(value: &str) -> ContractValue {
    ContractValue::String(value.to_owned())
}

fn object(entries: Vec<(&str, ContractValue)>) -> ContractValue {
    ContractValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect(),
    )
}

fn origin() -> ContractValue {
    object(vec![("source", text("user"))])
}

fn draft() -> ContractValue {
    object(vec![
        ("schema", text(DRAFT_REQUEST_SCHEMA)),
        ("id", text("cor-1")),
        (
            "target",
            object(vec![
                ("kind", text("source")),
                ("id", text("src-1")),
                ("version", ContractValue::Integer(1)),
            ]),
        ),
        ("statement", text("The locator is https://example.test/2.")),
        ("reason", text("Mistyped locator.")),
        ("origin", origin()),
    ])
}

/// Examples keyed by contract identifier and scenario name.
pub fn contract_examples() -> BTreeMap<String, BTreeMap<String, Example>> {
    let sample = |id: &str, value: ContractValue| {
        (
            id.to_owned(),
            BTreeMap::from([(
                "default".to_owned(),
                Example {
                    valid: true,
                    example: value,
                },
            )]),
        )
    };
    BTreeMap::from([
        sample(
            SOURCE_CREATE_SCHEMA,
            object(vec![
                ("schema", text(SOURCE_CREATE_SCHEMA)),
                ("id", text("src-1")),
                ("title", text("Issue 12")),
                ("locator", text("https://example.test/1")),
                (
                    "content",
                    object(vec![
                        ("kind", text("text")),
                        ("text", text("Original issue body.")),
                    ]),
                ),
                ("origin", object(vec![("source", text("external"))])),
            ]),
        ),
        sample(
            DISCUSSION_CREATE_SCHEMA,
            object(vec![
                ("schema", text(DISCUSSION_CREATE_SCHEMA)),
                ("id", text("disc-1")),
                ("original_request", text("Add export to the report page.")),
                ("source_ids", ContractValue::Array(vec![text("src-1")])),
            ]),
        ),
        sample(
            DISCUSSION_ROUND_SCHEMA,
            object(vec![
                ("schema", text(DISCUSSION_ROUND_SCHEMA)),
                ("id", text("disc-1")),
                ("expected_version", ContractValue::Integer(1)),
                (
                    "ask",
                    object(vec![
                        ("id", text("q1")),
                        ("text", text("Which format?")),
                        (
                            "options",
                            ContractValue::Array(vec![
                                object(vec![("id", text("1")), ("text", text("CSV"))]),
                                object(vec![("id", text("2")), ("text", text("PDF"))]),
                            ]),
                        ),
                        ("recommendation", text("1")),
                        ("reason", text("Simplest to produce.")),
                    ]),
                ),
            ]),
        ),
        sample(DRAFT_REQUEST_SCHEMA, draft()),
        sample(
            TASK_CREATE_SCHEMA,
            object(vec![
                ("schema", text(TASK_CREATE_SCHEMA)),
                ("id", text("task-1")),
                ("title", text("Report export")),
                ("goal", text("Users can export the report.")),
                (
                    "requirements",
                    ContractValue::Array(vec![text("Export as PDF.")]),
                ),
                ("decisions", ContractValue::Array(vec![text("PDF only.")])),
                (
                    "criteria",
                    ContractValue::Array(vec![object(vec![
                        ("id", text("c1")),
                        ("text", text("The PDF opens and matches the page.")),
                        ("required", ContractValue::Boolean(true)),
                    ])]),
                ),
                ("discussion_ids", ContractValue::Array(vec![text("disc-1")])),
            ]),
        ),
        sample(
            TASK_REVISE_SCHEMA,
            object(vec![
                ("schema", text(TASK_REVISE_SCHEMA)),
                ("id", text("task-1")),
                ("expected_version", ContractValue::Integer(1)),
                ("reason", text("The user also wants CSV.")),
                (
                    "requirements",
                    ContractValue::Array(vec![text("Export as PDF and CSV.")]),
                ),
                ("reconfirm_requirement", ContractValue::Boolean(true)),
            ]),
        ),
        sample(
            TASK_PREVIEW_SCHEMA,
            object(vec![
                ("schema", text(TASK_PREVIEW_SCHEMA)),
                (
                    "draft",
                    object(vec![
                        ("operation", text("create")),
                        (
                            "request",
                            object(vec![
                                ("schema", text(TASK_CREATE_SCHEMA)),
                                ("id", text("task-1")),
                                ("title", text("Report export")),
                                ("goal", text("Users can export the report.")),
                                (
                                    "requirements",
                                    ContractValue::Array(vec![text("Export as PDF.")]),
                                ),
                                (
                                    "criteria",
                                    ContractValue::Array(vec![object(vec![
                                        ("id", text("c1")),
                                        ("text", text("The PDF opens.")),
                                    ])]),
                                ),
                                ("discussion_ids", ContractValue::Array(vec![text("disc-1")])),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        sample(
            TASK_APPLY_SCHEMA,
            object(vec![
                ("schema", text(TASK_APPLY_SCHEMA)),
                (
                    "draft",
                    object(vec![
                        ("operation", text("create")),
                        (
                            "request",
                            object(vec![
                                ("schema", text(TASK_CREATE_SCHEMA)),
                                ("id", text("task-1")),
                                ("title", text("Report export")),
                                ("goal", text("Users can export the report.")),
                                (
                                    "requirements",
                                    ContractValue::Array(vec![text("Export as PDF.")]),
                                ),
                                (
                                    "criteria",
                                    ContractValue::Array(vec![object(vec![
                                        ("id", text("c1")),
                                        ("text", text("The PDF opens.")),
                                    ])]),
                                ),
                                ("discussion_ids", ContractValue::Array(vec![text("disc-1")])),
                            ]),
                        ),
                    ]),
                ),
                (
                    "approved_candidate",
                    text("<exact candidate from task preview>"),
                ),
                (
                    "reviewed_dependencies",
                    ContractValue::Array(vec![object(vec![
                        ("kind", text("discussion")),
                        ("id", text("disc-1")),
                        ("version", ContractValue::Integer(1)),
                    ])]),
                ),
            ]),
        ),
        sample(
            TASK_COMPLETE_SCHEMA,
            object(vec![
                ("schema", text(TASK_COMPLETE_SCHEMA)),
                ("id", text("task-1")),
                ("expected_version", ContractValue::Integer(1)),
            ]),
        ),
        sample(
            EXECUTION_RECORD_SCHEMA,
            object(vec![
                ("schema", text(EXECUTION_RECORD_SCHEMA)),
                ("id", text("exec-1")),
                ("task_id", text("task-1")),
                (
                    "attempt",
                    object(vec![
                        ("id", text("attempt-1")),
                        ("result", text("succeeded")),
                        (
                            "changes",
                            ContractValue::Array(vec![text("Added the export button.")]),
                        ),
                        (
                            "verification",
                            ContractValue::Array(vec![text("Ran the export test.")]),
                        ),
                    ]),
                ),
            ]),
        ),
        sample(
            TASK_ACCEPTANCE_RECORD_SCHEMA,
            object(vec![
                ("schema", text(TASK_ACCEPTANCE_RECORD_SCHEMA)),
                ("id", text("acc-1")),
                ("task_id", text("task-1")),
                ("task_version", ContractValue::Integer(1)),
                (
                    "conclusions",
                    ContractValue::Array(vec![object(vec![
                        ("criterion_id", text("c1")),
                        ("verdict", text("passed")),
                        ("evidence", text("Opened the exported PDF.")),
                        ("origin", object(vec![("source", text("host"))])),
                    ])]),
                ),
            ]),
        ),
        sample(
            REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA,
            object(vec![
                ("schema", text(REQUIREMENT_ACCEPTANCE_RECORD_SCHEMA)),
                ("id", text("req-acc-1")),
                ("discussion_id", text("disc-1")),
                ("verdict", text("passed")),
                ("evidence", text("The user confirmed the result.")),
                ("origin", origin()),
                (
                    "task_refs",
                    ContractValue::Array(vec![object(vec![
                        ("kind", text("task")),
                        ("id", text("task-1")),
                        ("version", ContractValue::Integer(2)),
                    ])]),
                ),
            ]),
        ),
        sample(
            APPLY_REQUEST_SCHEMA,
            object(vec![
                ("schema", text(APPLY_REQUEST_SCHEMA)),
                ("draft", draft()),
                ("approved_candidate", text("<candidate text from preview>")),
                ("reviewed_target_version", ContractValue::Integer(1)),
            ]),
        ),
    ])
}
