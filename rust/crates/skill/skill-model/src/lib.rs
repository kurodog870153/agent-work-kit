//! Shared Skill data shapes independent of a consuming domain.

pub mod hierarchy;
pub mod instruction;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn verified<T: serde::de::DeserializeOwned>(value: Value) -> Value {
    let _: T = serde_json::from_value(value.clone()).expect("Skill output matches its model");
    value
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillIdentity {
    pub scope: String,
    pub root: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillBundleFile {
    pub path: String,
    pub normalization: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillBundle<S> {
    pub schema: S,
    pub files: Vec<SkillBundleFile>,
    pub bundle_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillCatalog<S> {
    pub schema: S,
    pub skills: Vec<Value>,
    pub unavailable: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillSnapshot<S> {
    pub schema: S,
    pub skill: Value,
    pub bundle: SkillBundle<S>,
}

#[cfg(test)]
mod tests {
    use super::{SkillBundle, SkillCatalog, SkillIdentity, SkillMetadata, SkillSnapshot};
    use serde_json::json;

    #[test]
    fn shared_shapes_round_trip_and_reject_unknown_fields() {
        let identity = json!({"scope":"repo","root":"skills","source":"example/SKILL.md"});
        let typed: SkillIdentity = serde_json::from_value(identity.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), identity);
        let mut invalid = identity;
        invalid["extra"] = json!(true);
        assert!(serde_json::from_value::<SkillIdentity>(invalid).is_err());

        let metadata = json!({"name":"example","description":"A skill","tags":["analysis"]});
        let typed: SkillMetadata = serde_json::from_value(metadata.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), metadata);
        let mut invalid = metadata;
        invalid["work-tags"] = json!(["legacy"]);
        assert!(serde_json::from_value::<SkillMetadata>(invalid).is_err());

        let bundle = json!({"schema":"skill-bundle","files":[{"path":"SKILL.md","normalization":"raw","content_sha256":"abc"}],"bundle_sha256":"def"});
        let typed: SkillBundle<String> = serde_json::from_value(bundle.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), bundle);
        let mut invalid = bundle;
        invalid["files"][0]["extra"] = json!(true);
        assert!(serde_json::from_value::<SkillBundle<String>>(invalid).is_err());

        let catalog = json!({"schema":"skill-catalog","skills":[],"unavailable":[]});
        let typed: SkillCatalog<String> = serde_json::from_value(catalog.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), catalog);
        let mut invalid = catalog;
        invalid["extra"] = json!(true);
        assert!(serde_json::from_value::<SkillCatalog<String>>(invalid).is_err());

        let snapshot = json!({"schema":"skill-snapshot","skill":{},"bundle":{"schema":"skill-bundle","files":[],"bundle_sha256":"def"}});
        let typed: SkillSnapshot<String> = serde_json::from_value(snapshot.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), snapshot);
        let mut invalid = snapshot;
        invalid["bundle"]["extra"] = json!(true);
        assert!(serde_json::from_value::<SkillSnapshot<String>>(invalid).is_err());
    }
}
