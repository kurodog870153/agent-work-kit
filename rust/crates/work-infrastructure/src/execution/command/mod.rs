//! Host command execution and verification of retained command evidence.
pub(super) mod evidence;
pub(super) mod runner;

use serde_json::{Value, json};
use work_feature::error::{ExitCode, WorkError};
use work_feature::execution::ExecutionWriterContext;
use work_operations::derivation::fingerprint;

/// Declared project inputs are distinct from immutable requirement Sources and
/// from this Attempt's modification history. Directories are ranges, not trees.
pub(super) fn input_files(context: &ExecutionWriterContext) -> Result<Value, WorkError> {
    let collection = &context.task_context().collection;
    let task = collection["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|task| task["id"] == context.target().task_id)
        .ok_or_else(input_error)?;
    if work_operations::execution::file_transaction::input_paths(collection, task)
        .map_err(|_| input_error())?
        .is_empty()
    {
        return Ok(Value::Null);
    }
    let index_path = crate::files::resolve_runtime_path(
        &context.writer().canonical_project_root,
        &format!("{}/index.json", context.target().execution_dir),
    )?;
    let raw = super::evidence_file::read(&index_path, 256 * 1024 * 1024)?;
    let index = work_operations::canonical::parse_json_contract(&raw).map_err(|_| input_error())?;
    work_operations::execution::index::validate_execution_index(&index, &raw)
        .map_err(|_| input_error())?;
    let attempt = index["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["id"] == context.target().task_id)
        .and_then(|row| row["latest_attempt"].as_str())
        .ok_or_else(input_error)?;
    let paths = super::file_transaction::storage::resolved_input_paths(context, task, attempt)?;
    snapshot_paths(&context.writer().canonical_project_root, paths)
}

fn input_error() -> WorkError {
    WorkError::new(
        ExitCode::ArtifactIntegrity,
        "command_input_unverified",
        "A declared command input could not be verified; preserve execution evidence.",
        json!({}),
    )
}

#[cfg(test)]
fn snapshot_inputs(
    root: &std::path::Path,
    collection: &Value,
    task: &Value,
) -> Result<Value, WorkError> {
    snapshot_paths(
        root,
        work_operations::execution::file_transaction::input_paths(collection, task)
            .map_err(|_| input_error())?,
    )
}

fn snapshot_paths(root: &std::path::Path, paths: Vec<String>) -> Result<Value, WorkError> {
    use work_model::execution::response::CommandInputState;
    let mut inputs = std::collections::BTreeMap::new();
    for relative in paths {
        let path = crate::files::resolve_runtime_path(root, &relative)?;
        let state = match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => CommandInputState::Directory,
            Ok(metadata) if metadata.is_file() => CommandInputState::File {
                sha256: fingerprint::raw(&super::evidence_file::read(&path, 256 * 1024 * 1024)?),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                CommandInputState::Missing
            }
            _ => return Err(input_error()),
        };
        inputs.insert(relative, state);
    }
    // Preserve evidence without project inputs; an old receipt lacking a now
    // required input snapshot never compares equal to a nonempty snapshot.
    Ok(if inputs.is_empty() {
        Value::Null
    } else {
        json!(inputs)
    })
}

pub(super) fn acceptance_version(
    context: &ExecutionWriterContext,
    attempt: &str,
) -> Result<String, WorkError> {
    let history = super::file_transaction::storage::current_version(context, attempt)?;
    let inputs = input_files(context)?;
    Ok(if inputs.is_null() {
        history
    } else {
        fingerprint::structured(&json!({"file_version":history,"input_files":inputs}))
            .map_err(|_| input_error())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_input_snapshots_bind_output_bytes_but_not_directory_contents() {
        let root = std::env::temp_dir().join(format!(
            "work-input-version-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::write(root.join("src/output"), b"version A").unwrap();
        let collection = json!({"tasks":[{"id":"TASK-001","files":[{"id":"FILE-001","action":"move","source":"old","destination":"src/output"}]}]});
        let task = json!({"inputs":[{"kind":"project_state","source":"src"},{"kind":"task_output","source":"TASK-001/FILE-001"}]});
        let original = snapshot_inputs(&root, &collection, &task).unwrap();
        assert_eq!(original["src"], json!({"kind":"directory"}));
        assert_eq!(
            original["src/output"]["sha256"],
            fingerprint::raw(b"version A")
        );
        std::fs::write(root.join("src/unrelated-cache"), b"cache").unwrap();
        assert_eq!(
            original,
            snapshot_inputs(&root, &collection, &task).unwrap()
        );
        std::fs::write(root.join("src/output"), b"version B").unwrap();
        assert_ne!(
            original,
            snapshot_inputs(&root, &collection, &task).unwrap()
        );
        let missing = json!({"inputs":[{"kind":"project_state","source":"absent"}]});
        assert_eq!(
            snapshot_inputs(&root, &collection, &missing).unwrap()["absent"],
            json!({"kind":"missing"})
        );
    }
}
