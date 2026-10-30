//! Shared hierarchy catalog data, independent of formal selection.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct HierarchyCatalog {
    pub paths: Vec<String>,
    pub children: BTreeMap<String, Vec<String>>,
    pub metadata: BTreeMap<String, Value>,
    pub catalog_sha256: String,
}
