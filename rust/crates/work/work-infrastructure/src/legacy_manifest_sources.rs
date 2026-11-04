//! Retained instruction manifest source fingerprints and drift checks.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, PathBuf};

#[cfg(test)]
use serde_json::Value;
use serde_json::json;
use work_feature::error::{ExitCode, WorkError};
use work_feature::instruction::LegacyManifestSources;
use work_operations::derivation::fingerprint;

pub struct LegacyManifestSourceSession {
    root: PathBuf,
    sources: BTreeMap<String, (String, String, String)>,
}

impl LegacyManifestSourceSession {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            sources: BTreeMap::new(),
        }
    }

    fn read(&self, logical_name: &str, relative: &str) -> Result<Vec<u8>, WorkError> {
        let path = std::path::Path::new(relative);
        if path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(WorkError::new(
                ExitCode::ArtifactIntegrity,
                "routed_instruction_source_unsafe",
                "A retained instruction source path is unsafe.",
                json!({"logical_name":logical_name}),
            ));
        }
        let root = fs::canonicalize(&self.root).map_err(|_| {
            WorkError::new(
                ExitCode::ArtifactIntegrity,
                "routed_instruction_root_missing",
                "The retained instruction root is unavailable.",
                json!({}),
            )
        })?;
        let target = fs::canonicalize(root.join(path)).map_err(|_| {
            WorkError::new(
                ExitCode::ArtifactIntegrity,
                "routed_instruction_source_missing",
                "A retained instruction source is missing or unreadable.",
                json!({"logical_name":logical_name,"path":relative}),
            )
        })?;
        if !target.starts_with(&root) || !target.is_file() {
            return Err(WorkError::new(
                ExitCode::ArtifactIntegrity,
                "routed_instruction_source_unsafe",
                "A retained instruction source escapes its Skill root.",
                json!({"logical_name":logical_name}),
            ));
        }
        fs::read(target).map_err(|_| {
            WorkError::new(
                ExitCode::ArtifactIntegrity,
                "routed_instruction_source_missing",
                "A routed instruction source is missing or unreadable.",
                json!({"logical_name":logical_name,"path":relative}),
            )
        })
    }

    pub fn fingerprint(&mut self, logical_name: &str, relative: &str) -> Result<String, WorkError> {
        if let Some((stored_path, _, canonical)) = self.sources.get(logical_name)
            && stored_path == relative
        {
            return Ok(canonical.clone());
        }
        let raw = self.read(logical_name, relative)?;
        let canonical = fingerprint::canonical(&raw).map_err(|_| {
            WorkError::new(
                ExitCode::InputFormat,
                "invalid_utf8",
                "The routed instruction source is not valid UTF-8.",
                json!({"logical_name":logical_name,"path":relative}),
            )
        })?;
        self.sources.insert(
            logical_name.into(),
            (relative.into(), fingerprint::raw(&raw), canonical.clone()),
        );
        Ok(canonical)
    }

    pub fn recheck(&self) -> Result<(), WorkError> {
        for (logical_name, (relative, expected_raw, _)) in &self.sources {
            if fingerprint::raw(&self.read(logical_name, relative)?) != *expected_raw {
                return Err(WorkError::new(
                    ExitCode::ArtifactIntegrity,
                    "routed_instruction_source_changed",
                    "A routed instruction source changed during routing construction.",
                    json!({"logical_name":logical_name}),
                ));
            }
        }
        Ok(())
    }
}

impl LegacyManifestSources for LegacyManifestSourceSession {
    fn canonical_source_sha256(&mut self, name: &str, path: &str) -> Result<String, WorkError> {
        self.fingerprint(name, path)
    }

    fn recheck_sources(&self) -> Result<(), WorkError> {
        self.recheck()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use work_feature::workflow::{execution_state, pre_execution_state};

    #[test]
    fn retained_manifest_rechecks_saved_sources_without_selecting_a_route() {
        let root = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/work"
        ));
        let mut session = LegacyManifestSourceSession::new(root);
        let stored = work_model::contract::data::registry_value()["items"]["work-instruction-selection-manifest"]["description"]["example"].clone();
        let current = work_feature::instruction::refresh_legacy_manifest(
            &mut session,
            &stored,
            &json!({"schema":"work-task-index","requirement_id":"example"}),
        )
        .unwrap();
        assert!(work_operations::protocol::valid_sha256(
            current["selection_sha256"].as_str().unwrap()
        ));
        assert!(!current["sources"].as_array().unwrap().is_empty());
        let mut tampered = stored;
        tampered["sources"][0]["path"] = json!("../outside.md");
        assert_eq!(
            work_feature::instruction::refresh_legacy_manifest(
                &mut session,
                &tampered,
                &json!({"schema":"work-task-index","requirement_id":"example"}),
            )
            .unwrap_err()
            .reason_code,
            "instruction_selection_manifest_drift"
        );
    }

    #[test]
    fn source_session_matches_current_contract_identity_and_rejects_drift() {
        let temp = std::env::temp_dir().join(format!("work-routing-{}", std::process::id()));
        fs::create_dir(&temp).unwrap();
        let mut absent = LegacyManifestSourceSession::new(temp.clone());
        assert_eq!(
            absent
                .fingerprint("escape", "../outside.md")
                .unwrap_err()
                .reason_code,
            "routed_instruction_source_unsafe"
        );
        assert_eq!(
            absent
                .fingerprint("missing", "missing.md")
                .unwrap_err()
                .reason_code,
            "routed_instruction_source_missing"
        );
        let source = temp.join("source.md");
        fs::write(&source, b"first\n").unwrap();
        let mut changed = LegacyManifestSourceSession::new(temp.clone());
        changed.fingerprint("source", "source.md").unwrap();
        fs::write(&source, b"second\n").unwrap();
        assert_eq!(
            changed.recheck().unwrap_err().reason_code,
            "routed_instruction_source_changed"
        );
        fs::remove_file(source).unwrap();
        fs::remove_dir(temp).unwrap();
    }

    #[test]
    fn source_required_workflow_state_exposes_formal_status_only() {
        let artifacts = json!({"source":"outputs/work/sources/demo","task":"outputs/work/tasks/demo/index.json","execution":"outputs/work/executions/demo"});
        let state = pre_execution_state("demo", &artifacts, None, None, None, false)
            .unwrap()
            .unwrap();
        assert_eq!(state["status"], "source_required");
        assert!(state.get("next_action").is_none());
        assert!(state.get("selection_manifest").is_none());
        assert!(state.get("command").is_none());
    }

    #[test]
    fn execution_workflow_states_match_current_contract_transitions() {
        let repo = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."));
        let fixture =
            repo.join("crates/work/work-infrastructure/fixtures/cases/execution/workflow-state");
        for name in [
            "pending",
            "in-progress",
            "retry",
            "completed",
            "invalid-lock",
            "reconciliation",
            "reconciled",
        ] {
            let input: Value = serde_json::from_slice(
                &fs::read(fixture.join(format!("{name}/input/state.json"))).unwrap(),
            )
            .unwrap();
            let expected: Value = serde_json::from_slice(
                &fs::read(fixture.join(format!("{name}/expected/result.json"))).unwrap(),
            )
            .unwrap();
            let actual = execution_state(
                input["requirement_id"].as_str().unwrap(),
                &input["artifacts"],
                &input["execution_index"],
                &input["latest_attempts"],
            )
            .unwrap();
            assert_eq!(actual, expected, "workflow state {name}");
        }
    }
}
