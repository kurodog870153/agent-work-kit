//! Filesystem skill bundle snapshots.

use std::path::Path;

#[cfg(test)]
use std::fs;

use serde_json::{Value, json};
use work_feature::error::{ExitCode, WorkError};
use work_operations::derivation::fingerprint;
use work_operations::skill::BundleEntry;

fn error(code: ExitCode, reason: &str, message: &str, details: Value) -> WorkError {
    WorkError::new(code, reason, message, details)
}

pub fn snapshot_skill_bundle(skill_root: &Path) -> Result<Value, WorkError> {
    use skill_api::bundle::BundleReadError;
    let files =
        skill_api::bundle::read_bundle_files(skill_root).map_err(|failure| match failure {
            BundleReadError::RootResolution(path) => error(
                ExitCode::IoFailure,
                "skill_root_resolution_failed",
                "The skill root could not be resolved.",
                json!({"path": path}),
            ),
            BundleReadError::RootNotDirectory(path) => error(
                ExitCode::IoFailure,
                "skill_root_not_directory",
                "The skill root is not a directory.",
                json!({"path": path}),
            ),
            BundleReadError::PathResolution(path) => error(
                ExitCode::IoFailure,
                "skill_bundle_path_resolution_failed",
                "A skill bundle path could not be resolved.",
                json!({"path": path}),
            ),
            BundleReadError::PathEscapesRoot { path, resolved } => error(
                ExitCode::ArtifactIntegrity,
                "skill_bundle_path_escapes_root",
                "A skill bundle path resolves outside the skill root.",
                json!({"path": path, "resolved_path": resolved}),
            ),
            BundleReadError::FileRead(path) => error(
                ExitCode::IoFailure,
                "file_read_failed",
                "The file could not be read.",
                json!({"path": path}),
            ),
            BundleReadError::InvalidUtf8 {
                source,
                byte_offset,
            } => error(
                ExitCode::InputFormat,
                "invalid_utf8",
                "The input is not valid UTF-8.",
                json!({"source": source, "byte_offset": byte_offset}),
            ),
        })?;
    let entries: Vec<_> = files
        .iter()
        .map(|file| BundleEntry {
            path: &file.path,
            normalization: &file.normalization,
            content_sha256: &file.content_sha256,
        })
        .collect();
    Ok(
        work_model::skill::verified::<work_model::skill::SkillBundle>(
            json!({"schema": "work-skill-bundle", "files": files,
            "bundle_sha256": fingerprint::skill_bundle(&entries)}),
        ),
    )
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_content_changes_bundle_fingerprint() {
        let root = std::env::temp_dir().join(format!(
            "work-skill-bundle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("references")).unwrap();
        fs::write(
            root.join("SKILL.md"),
            b"---\nname: frontend\ndescription: Frontend skill.\n---\n",
        )
        .unwrap();
        let reference = root.join("references/guide.md");
        fs::write(&reference, b"Version one\n").unwrap();
        let first = snapshot_skill_bundle(&root).unwrap();
        assert_eq!(
            first["bundle_sha256"],
            "e30ac49a17fcd6c9974890c9cac67eee80b5b200e3a6db4caaed5d1d4982388d"
        );
        fs::write(&reference, b"Version two\n").unwrap();
        let second = snapshot_skill_bundle(&root).unwrap();
        assert_ne!(first["bundle_sha256"], second["bundle_sha256"]);
    }

    #[test]
    fn current_rust_skill_bundle_has_expected_contents() {
        let root = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../../skills/work"
        ));
        let bundle = snapshot_skill_bundle(root).unwrap();
        assert_eq!(bundle["schema"], "work-skill-bundle");
        let files = bundle["files"].as_array().unwrap();
        assert!(files.iter().any(|file| file["path"] == "SKILL.md"));
        assert!(
            files
                .iter()
                .any(|file| file["path"] == "references/instruction-loading/source-loading.md")
        );
        assert!(work_operations::protocol::valid_sha256(
            bundle["bundle_sha256"].as_str().unwrap()
        ));
    }
}
