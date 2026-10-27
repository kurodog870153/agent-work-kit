//! Selected Skill format owned by Task planning and shared with Execution.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::schema::PublicSchema;

pub use skill_api::model::{SkillBundleFile, verified};
pub type SkillBundle = skill_api::model::SkillBundle<PublicSchema>;
pub type SkillCatalog = skill_api::model::SkillCatalog<PublicSchema>;
pub type SkillSnapshot = skill_api::model::SkillSnapshot<PublicSchema>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillValidationStatus {
    Valid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSelectionValidation {
    pub schema: PublicSchema,
    pub status: SkillValidationStatus,
    pub skill_selection: SkillSelection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillSelectionDecision {
    BaseOnly,
    ExternalSkills,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillModeSupport {
    Declared,
    Inferred,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedSkillModeSupport {
    pub task: SkillModeSupport,
    pub execute: SkillModeSupport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SelectedSkill {
    pub id: String,
    pub name: String,
    pub scope: String,
    pub root: String,
    pub source: String,
    pub description: String,
    pub mode_support: SelectedSkillModeSupport,
    pub allow_implicit_invocation: bool,
    pub dependency_status: String,
    pub summary_sha256: String,
    pub bundle_sha256: String,
    pub recommendation_reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSelection {
    pub schema: PublicSchema,
    pub decision: SkillSelectionDecision,
    pub skills: Vec<Value>,
    pub selection_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSelectionChoice {
    pub scope: String,
    pub root: String,
    pub source: String,
    pub recommendation_reason: String,
    pub dependency_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode_support: Option<SelectedSkillModeSupport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSelectionRequest {
    pub decision: SkillSelectionDecision,
    pub skills: Vec<SkillSelectionChoice>,
}

#[cfg(test)]
mod contract_tests {
    use super::*;

    #[test]
    fn public_skill_examples_match_models() {
        let registry: Value = crate::contract::data::registry_value();
        let items = &registry["items"];
        assert!(
            serde_json::from_value::<SkillBundle>(
                items["work-skill-bundle"]["description"]["example"].clone()
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<SkillCatalog>(
                items["work-skill-catalog"]["description"]["example"].clone()
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<SkillSelectionValidation>(
                items["work-skill-selection-validation"]["description"]["example"].clone()
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<SkillSelection>(
                items["work-skill-selection"]["description"]["example"].clone()
            )
            .is_ok()
        );
        assert!(
            serde_json::from_value::<SkillSnapshot>(
                items["work-skill-snapshot"]["description"]["example"].clone()
            )
            .is_ok()
        );
    }

    #[test]
    fn existing_bundle_and_snapshot_round_trip_without_schema_rewrite() {
        let registry: Value = crate::contract::data::registry_value();
        let items = &registry["items"];
        let bundle = items["work-skill-bundle"]["description"]["example"].clone();
        let snapshot = items["work-skill-snapshot"]["description"]["example"].clone();
        let parsed_bundle: SkillBundle = serde_json::from_value(bundle.clone()).unwrap();
        let parsed_snapshot: SkillSnapshot = serde_json::from_value(snapshot.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed_bundle).unwrap(), bundle);
        assert_eq!(serde_json::to_value(parsed_snapshot).unwrap(), snapshot);

        let mut damaged_bundle = bundle;
        damaged_bundle["files"] = serde_json::json!([{"path":"SKILL.md","normalization":"raw","content_sha256":"abc","unexpected":true}]);
        assert!(serde_json::from_value::<SkillBundle>(damaged_bundle).is_err());
        let mut damaged_snapshot = snapshot;
        damaged_snapshot["bundle"]["schema"] = serde_json::json!("skill-bundle");
        assert!(serde_json::from_value::<SkillSnapshot>(damaged_snapshot).is_err());
    }
}
