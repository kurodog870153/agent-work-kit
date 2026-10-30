//! Instruction source and catalog data shared across Skill consumers.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::SkillMetadata;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstructionFrontmatter {
    pub name: String,
    pub description: String,
    pub metadata: InstructionFrontmatterMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstructionFrontmatterMetadata {
    pub tags: Vec<String>,
}

impl From<InstructionFrontmatter> for SkillMetadata {
    fn from(value: InstructionFrontmatter) -> Self {
        Self {
            name: value.name,
            description: value.description,
            tags: value.metadata.tags,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstructionCatalogData {
    pub paths: Vec<String>,
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSummary {
    pub kind: String,
    pub logical_name: String,
    pub canonical_sha256: String,
}

#[derive(Debug, Clone)]
pub struct LoadedSource {
    pub summary: SourceSummary,
    pub canonical_content: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::SourceSummary;
    use serde_json::json;

    #[test]
    fn source_summary_round_trips_and_rejects_unknown_fields() {
        let source =
            json!({"kind":"instruction","logical_name":"general","canonical_sha256":"abc"});
        let typed: SourceSummary = serde_json::from_value(source.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), source);
        let mut invalid = source;
        invalid["raw_sha256"] = json!("def");
        assert!(serde_json::from_value::<SourceSummary>(invalid).is_err());
    }
}
