//! Declared examples of the Skill CLI request contracts.

use std::collections::BTreeMap;

use contract_core::model::{ContractValue, Example};

use crate::RESOLVE_REQUEST;

/// Examples keyed by contract identifier and scenario name.
pub fn contract_examples() -> BTreeMap<String, BTreeMap<String, Example>> {
    let object = |entries: Vec<(&str, ContractValue)>| {
        ContractValue::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_owned(), value))
                .collect(),
        )
    };
    let text = |value: &str| ContractValue::String(value.to_owned());
    let list = |values: Vec<ContractValue>| ContractValue::Array(values);
    let request = object(vec![
        ("schema", text(RESOLVE_REQUEST)),
        (
            "selections",
            list(vec![object(vec![
                ("skill", text("work")),
                ("nodes", list(vec![text("task")])),
            ])]),
        ),
    ]);
    BTreeMap::from([(
        RESOLVE_REQUEST.to_owned(),
        BTreeMap::from([(
            "default".to_owned(),
            Example {
                valid: true,
                example: request,
            },
        )]),
    )])
}
