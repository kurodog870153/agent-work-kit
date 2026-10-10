//! Embedded public contract descriptions and scaffold ordering.
use super::{ContractCatalogEntry, ContractRegistrySnapshot};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContractData {
    registry: ContractRegistrySnapshot,
    scaffold_orders: Value,
}

fn embedded() -> &'static ContractData {
    static DATA: OnceLock<ContractData> = OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../resources/contract/data.json"))
            .expect("embedded contract data matches its model")
    })
}

pub fn registry() -> ContractRegistrySnapshot {
    let mut registry = embedded().registry.clone();
    for (id, record) in crate::discussion::contracts::records()
        .into_iter()
        .chain(crate::execution::file_transaction::contracts::records())
    {
        registry.catalog.contracts.push(ContractCatalogEntry {
            id: id.clone(),
            kind: record.description.kind,
            caller_constructible: record.description.caller_constructible,
        });
        assert!(
            registry.items.insert(id, record).is_none(),
            "duplicate contract ID"
        );
    }
    registry.catalog.contracts.sort_by(|a, b| a.id.cmp(&b.id));
    registry
}

pub fn registry_value() -> Value {
    serde_json::to_value(registry()).expect("serializable contract registry")
}

pub fn scaffold_orders() -> Value {
    embedded().scaffold_orders.clone()
}
