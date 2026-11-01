//! Work data shapes and single-type invariants.

pub mod common;
pub mod contract;
pub mod discussion;
pub mod execution;
pub mod hierarchy;
pub mod identifiers;
pub mod instruction;
pub mod runtime;
pub mod schema;
pub mod skill;
pub mod source;
pub mod specification;
pub mod task;
pub mod workflow;

#[cfg(test)]
mod remaining_contract_examples {
    use crate::source;
    use serde::de::DeserializeOwned;
    use serde_json::Value;

    fn matches<T: DeserializeOwned>(items: &Value, id: &str) {
        serde_json::from_value::<T>(items[id]["description"]["example"].clone())
            .unwrap_or_else(|error| panic!("{id}: {error}"));
    }

    #[test]
    fn public_remaining_examples_match_models() {
        let registry: Value = crate::contract::data::registry_value();
        let items = &registry["items"];
        assert!(items.get("work-operation-envelope").is_none());
        assert!(items.get("work-operation-result").is_none());
        matches::<source::SourceImpact>(items, "work-source-impact");
    }
}
