//! Prepare a simultaneous final file tree from current snapshots; no target writes.
use super::{FileTransactionRepository, error};
use crate::error::WorkError;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use work_model::execution::attempt::ScopeModification;
use work_model::execution::file_transaction::{FileMetadata, FileState};
use work_model::runtime::{RuntimeBytes, RuntimeTarget};
use work_operations::canonical::portable_path_identity;
use work_operations::derivation::{fingerprint, identity};
use work_operations::execution::authorization;

use work_model::execution::file_transaction::FileChange as Change;

pub(super) struct PreparedFiles {
    pub targets: Vec<RuntimeTarget>,
    pub before_metadata: BTreeMap<String, Option<FileMetadata>>,
    pub after_metadata: BTreeMap<String, Option<FileMetadata>>,
    pub path_identities: BTreeMap<String, Option<String>>,
}
fn paths(change: &Change) -> Vec<&str> {
    match change {
        Change::Write { path, .. } => vec![path],
        Change::Move {
            source,
            destination,
            ..
        } => vec![source, destination],
    }
}
fn permitted(path: &str) -> bool {
    identity::runtime_relative_path(path)
        && path != ".git"
        && !path.starts_with(".git/")
        && path != "outputs/work"
        && !path.starts_with("outputs/work/")
}
pub(super) fn effective_collection(
    collection: &Value,
    accepted: &BTreeMap<String, Vec<String>>,
) -> Result<Value, WorkError> {
    let mut effective = collection.clone();
    for (owner, paths) in accepted {
        let task = effective["tasks"]
            .as_array_mut()
            .and_then(|tasks| tasks.iter_mut().find(|task| task["id"] == *owner))
            .ok_or_else(|| error("file_transaction_scope_owner_missing"))?;
        let mut files = task["files"].as_array().cloned().unwrap_or_default();
        for path in paths {
            if !files.iter().any(|file| {
                ["path", "source", "destination"]
                    .iter()
                    .any(|key| file[*key] == *path)
            }) {
                files.push(json!({"action":"modify","path":path}));
            }
        }
        task["files"] = json!(files);
    }
    Ok(effective)
}
pub(super) fn prepare(
    repository: &impl FileTransactionRepository,
    collection: &Value,
    task: &Value,
    attempt: &Value,
    changes: &[Change],
    mut new_metadata: impl FnMut(&str) -> Result<FileMetadata, WorkError>,
) -> Result<PreparedFiles, WorkError> {
    if changes.is_empty() {
        return Err(error("file_transaction_empty_change"));
    }
    let mut canonical = BTreeMap::new();
    let mut requested = BTreeSet::new();
    let mut moved = BTreeSet::new();
    let mut destinations = BTreeSet::new();
    let mut declared_bytes = 0_u64;
    for change in changes {
        for path in paths(change) {
            if !permitted(path) {
                return Err(error("file_transaction_target_unsupported"));
            }
            if canonical
                .insert(portable_path_identity(path), path.to_owned())
                .is_some_and(|old| old != path)
            {
                return Err(error("file_transaction_path_alias"));
            }
            requested.insert(path.to_owned());
        }
        let (destination, bytes) = match change {
            Change::Write { path, bytes } => (path, Some(bytes)),
            Change::Move {
                source,
                destination,
                bytes,
            } => {
                if source == destination || !moved.insert(source.clone()) {
                    return Err(error("file_transaction_move_source_duplicate"));
                }
                (destination, bytes.as_ref())
            }
        };
        if !destinations.insert(destination.clone()) {
            return Err(error("file_transaction_destination_duplicate"));
        }
        declared_bytes = declared_bytes
            .checked_add(bytes.map_or(0, |b| b.len() as u64))
            .ok_or_else(|| error("file_transaction_resource_limit"))?;
    }
    let requested: Vec<_> = requested.into_iter().collect();
    authorization::require_modified_files(attempt, &requested, None)
        .map_err(|e| error(e.reason_code))?;
    let scope = authorization::scope::from_attempt(attempt).map_err(|e| error(e.reason_code))?;
    // Add supplemental writes only for conflict analysis; never invent dependency edges.
    let mut effective = collection.clone();
    let current = effective["tasks"]
        .as_array_mut()
        .and_then(|tasks| tasks.iter_mut().find(|row| row["id"] == task["id"]))
        .ok_or_else(|| error("file_transaction_task_missing"))?;
    let mut files = current["files"].as_array().cloned().unwrap_or_default();
    for path in &requested {
        if !files.iter().any(|file| {
            ["path", "source", "destination"]
                .iter()
                .any(|key| file[*key] == *path)
        }) {
            files.push(json!({"action":"modify","path":path}));
        }
    }
    current["files"] = json!(files);
    work_operations::task::file_dependencies::validate(&effective)
        .map_err(|e| error(e.reason_code))?;
    repository.check_resources(&requested)?;
    repository.check_complete_resources(declared_bytes)?;
    let mut before = BTreeMap::new();
    let mut path_identities = BTreeMap::new();
    let mut physical = BTreeSet::new();
    for path in &requested {
        let state = repository.inspect(path)?;
        let id = repository.path_identity(path)?;
        if state.is_some() != id.is_some()
            || id.as_ref().is_some_and(|id| !physical.insert(id.clone()))
        {
            return Err(error("file_transaction_path_identity"));
        }
        before.insert(path.clone(), state);
        path_identities.insert(path.clone(), id);
    }
    let mut weighted = requested.iter().try_fold(declared_bytes, |sum, path| {
        sum.checked_add(path.len() as u64 + 4096)
            .and_then(|sum| {
                sum.checked_add(before[path].as_ref().map_or(0, |s| s.bytes.len() as u64))
            })
            .ok_or_else(|| error("file_transaction_resource_limit"))
    })?;
    let files: Vec<_> = changes
        .iter()
        .map(|change| match change {
            Change::Write { path, .. } => {
                json!({"action":if before[path].is_some() {"modify"} else {"create"},"path":path})
            }
            Change::Move {
                source,
                destination,
                ..
            } => json!({"action":"move","source":source,"destination":destination}),
        })
        .collect();
    let initial = before
        .iter()
        .map(|(path, state)| (portable_path_identity(path), state.is_some()))
        .collect();
    work_operations::task::file_state::final_state(&files, &initial, portable_path_identity)
        .map_err(|e| {
            error(match e.kind {
                "move_source_missing" => "file_transaction_move_source_missing",
                "move_destination_exists" => "file_transaction_move_destination_exists",
                _ => "file_transaction_file_set_conflict",
            })
        })?;
    for change in changes {
        if let Change::Move { source, bytes, .. } = change {
            let original = before[source]
                .as_ref()
                .ok_or_else(|| error("file_transaction_move_source_missing"))?;
            if bytes.is_none() {
                weighted = weighted
                    .checked_add(original.bytes.len() as u64)
                    .ok_or_else(|| error("file_transaction_resource_limit"))?;
            }
        }
    }
    repository.check_complete_resources(weighted)?;
    for change in changes {
        let kind = match change {
            Change::Write { path, .. } => {
                if before[path].is_some() {
                    ScopeModification::Modify
                } else {
                    ScopeModification::Create
                }
            }
            Change::Move { source, bytes, .. } => {
                let changes_content = bytes.as_ref().is_some_and(|bytes| {
                    bytes != &before[source].as_ref().expect("move sources checked").bytes
                });
                if changes_content
                    && scope.as_ref().is_some_and(|scope| {
                        !scope
                            .modification_types
                            .contains(&ScopeModification::Modify)
                    })
                {
                    return Err(error("execution_authorization_scope_expansion"));
                }
                ScopeModification::Move
            }
        };
        if scope
            .as_ref()
            .is_some_and(|scope| !scope.modification_types.contains(&kind))
        {
            return Err(error("execution_authorization_scope_expansion"));
        }
    }
    let mut after = BTreeMap::<String, Option<FileState>>::new();
    for source in &moved {
        after.insert(source.clone(), None);
    }
    for change in changes {
        let (path, state) = match change {
            Change::Write { path, bytes } => {
                let metadata = match before[path].as_ref() {
                    Some(state) => state.metadata.clone(),
                    None => new_metadata(path)?,
                };
                (
                    path,
                    FileState {
                        bytes: bytes.clone(),
                        metadata,
                    },
                )
            }
            Change::Move {
                source,
                destination,
                bytes,
            } => {
                let original = before[source].as_ref().expect("move sources checked");
                (
                    destination,
                    FileState {
                        bytes: bytes.as_ref().unwrap_or(&original.bytes).clone(),
                        metadata: original.metadata.clone(),
                    },
                )
            }
        };
        after.insert(path.clone(), Some(state));
    }
    let mut targets = Vec::new();
    let mut before_metadata = BTreeMap::new();
    let mut after_metadata = BTreeMap::new();
    for path in &requested {
        // Check both physical identity and current bytes/metadata after the entire set is prepared.
        if repository.inspect(path)? != before[path]
            || repository.path_identity(path)? != path_identities[path]
        {
            return Err(error("file_transaction_source_drift"));
        }
    }
    let raw = |state: FileState| RuntimeBytes {
        sha256: fingerprint::raw(&state.bytes),
        bytes: state.bytes,
    };
    for path in requested {
        let original = before.remove(&path).expect("complete before inventory");
        let result = after
            .remove(&path)
            .ok_or_else(|| error("file_transaction_incomplete_set"))?;
        before_metadata.insert(
            path.clone(),
            original.as_ref().map(|state| state.metadata.clone()),
        );
        after_metadata.insert(
            path.clone(),
            result.as_ref().map(|state| state.metadata.clone()),
        );
        targets.push(RuntimeTarget {
            path,
            before: original.map(raw),
            after: result.map(raw),
        });
    }
    Ok(PreparedFiles {
        targets,
        before_metadata,
        after_metadata,
        path_identities,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::ExecutionWriterContext;
    use std::cell::Cell;
    use work_model::execution::file_transaction::{
        FileRecoveryPreview, FileTransactionPreview, FileTransactionResult,
    };
    fn metadata() -> FileMetadata {
        FileMetadata {
            readonly: false,
            unix_mode: Some(0o100644),
            unix_uid: Some(501),
            unix_gid: Some(20),
            windows_security: None,
            macos_xattrs: Some(BTreeMap::new()),
        }
    }
    struct Memory {
        files: BTreeMap<String, FileState>,
        reads: Cell<usize>,
        identities: Cell<usize>,
        drift: bool,
        identity_drift: bool,
        deny_capacity: bool,
        deny_path: bool,
    }
    impl Memory {
        fn new() -> Self {
            Self {
                files: BTreeMap::from([
                    (
                        "src/a".into(),
                        FileState {
                            bytes: b"uncommitted a".to_vec(),
                            metadata: metadata(),
                        },
                    ),
                    (
                        "src/b".into(),
                        FileState {
                            bytes: b"previous successful b".to_vec(),
                            metadata: metadata(),
                        },
                    ),
                ]),
                reads: Cell::new(0),
                identities: Cell::new(0),
                drift: false,
                identity_drift: false,
                deny_capacity: false,
                deny_path: false,
            }
        }
    }
    impl FileTransactionRepository for Memory {
        fn check_resources(&self, _: &[String]) -> Result<(), WorkError> {
            if self.deny_path {
                Err(error("file_transaction_path_alias"))
            } else {
                Ok(())
            }
        }
        fn check_complete_resources(&self, _: u64) -> Result<(), WorkError> {
            if self.deny_capacity {
                Err(error("file_transaction_storage_insufficient"))
            } else {
                Ok(())
            }
        }
        fn path_identity(&self, path: &str) -> Result<Option<String>, WorkError> {
            let count = self.identities.get() + 1;
            self.identities.set(count);
            Ok(self
                .files
                .contains_key(path)
                .then(|| format!("{path}-{}", usize::from(self.identity_drift && count > 1))))
        }
        fn inspect(&self, path: &str) -> Result<Option<FileState>, WorkError> {
            let count = self.reads.get() + 1;
            self.reads.set(count);
            let mut value = self.files.get(path).cloned();
            if self.drift
                && count > 1
                && let Some(state) = &mut value
            {
                state.bytes = b"foreign change".to_vec();
            }
            Ok(value)
        }
        fn read_source(&self, _: &str) -> Result<Vec<u8>, WorkError> {
            unreachable!("candidate only inspects")
        }
        fn execute_selection(&self, _: &Value, _: &Value) -> Result<Value, WorkError> {
            unreachable!()
        }
        fn check_prepare(&self, _: &ExecutionWriterContext) -> Result<(), WorkError> {
            unreachable!()
        }
        fn apply(
            &self,
            _: &ExecutionWriterContext,
            _: &FileTransactionPreview,
            _: &str,
        ) -> Result<FileTransactionResult, WorkError> {
            unreachable!("preview cannot write")
        }
        fn recovery_preview(
            &self,
            _: &ExecutionWriterContext,
            _: &str,
        ) -> Result<FileRecoveryPreview, WorkError> {
            unreachable!()
        }
        fn restore(
            &self,
            _: &ExecutionWriterContext,
            _: &FileRecoveryPreview,
            _: &str,
        ) -> Result<FileTransactionResult, WorkError> {
            unreachable!("preview cannot restore")
        }
    }
    fn task() -> Value {
        json!({"id":"TASK-001","dependencies":[],"files":[{"action":"modify","path":"src/a"}]})
    }
    fn plan(task: &Value) -> Value {
        json!({"tasks":[task]})
    }
    fn authority() -> Value {
        json!({"authorization":{"task_id":"TASK-001","modifiable_files":["src/a","src/b","src/c"]},"execution_deviations":[]})
    }
    fn build(memory: &Memory, changes: &[Change]) -> Result<PreparedFiles, WorkError> {
        let task = task();
        prepare(memory, &plan(&task), &task, &authority(), changes, |_| {
            Ok(metadata())
        })
    }
    fn write(path: &str, bytes: &[u8]) -> Change {
        Change::Write {
            path: path.into(),
            bytes: bytes.to_vec(),
        }
    }
    fn mv(source: &str, destination: &str, bytes: Option<&[u8]>) -> Change {
        Change::Move {
            source: source.into(),
            destination: destination.into(),
            bytes: bytes.map(<[u8]>::to_vec),
        }
    }
    #[test]
    fn current_dirty_and_previous_successful_bytes_are_the_only_before_images() {
        let mut memory = Memory::new();
        let first = build(&memory, &[write("src/a", b"first success")]).unwrap();
        assert_eq!(
            first.targets[0].before.as_ref().unwrap().bytes,
            b"uncommitted a"
        );
        assert_eq!(first.before_metadata["src/a"], Some(metadata()));
        assert_eq!(first.path_identities["src/a"], Some("src/a-0".into()));
        assert_eq!(
            memory.files["src/a"].bytes, b"uncommitted a",
            "preview cannot change targets"
        );
        memory.files.get_mut("src/a").unwrap().bytes = b"first success".to_vec();
        let second = build(
            &memory,
            &[write("src/a", b"second"), write("src/b", b"third")],
        )
        .unwrap();
        assert_eq!(
            second.targets[0].before.as_ref().unwrap().bytes,
            b"first success"
        );
        assert_eq!(
            second.targets[1].before.as_ref().unwrap().bytes,
            b"previous successful b"
        );
    }
    #[test]
    fn move_modified_destination_and_old_path_replacement_share_one_initial_tree() {
        let memory = Memory::new();
        let a = build(
            &memory,
            &[
                mv("src/a", "src/c", Some(b"edited move")),
                write("src/a", b"replacement"),
            ],
        )
        .unwrap();
        let b = build(
            &memory,
            &[
                write("src/a", b"replacement"),
                mv("src/a", "src/c", Some(b"edited move")),
            ],
        )
        .unwrap();
        assert_eq!(a.targets, b.targets);
        assert_eq!(a.targets[0].after.as_ref().unwrap().bytes, b"replacement");
        assert_eq!(a.targets[1].before, None);
        assert_eq!(a.targets[1].after.as_ref().unwrap().bytes, b"edited move");
        assert_eq!(a.after_metadata["src/c"], Some(metadata()));
        let swap = build(
            &memory,
            &[mv("src/a", "src/b", None), mv("src/b", "src/a", None)],
        )
        .unwrap();
        assert_eq!(
            swap.targets[0].after.as_ref().unwrap().bytes,
            b"previous successful b"
        );
        assert_eq!(
            swap.targets[1].after.as_ref().unwrap().bytes,
            b"uncommitted a"
        );
    }
    #[test]
    fn aliases_ambiguous_requests_missing_sources_and_occupied_destinations_fail_before_writes() {
        let memory = Memory::new();
        for changes in [
            vec![write("src/a", b"a"), write("src/A", b"b")],
            vec![write("src/c", b"a"), mv("src/a", "src/c", None)],
            vec![mv("src/c", "src/a", None)],
            vec![mv("src/a", "src/b", None)],
            vec![write("../outside", b"x")],
            vec![mv("src/a", "src/a", None)],
        ] {
            assert!(build(&memory, &changes).is_err());
        }
        assert_eq!(memory.files["src/a"].bytes, b"uncommitted a");
    }
    #[test]
    fn drift_in_bytes_or_physical_identity_and_unavailable_capacity_or_paths_are_rejected() {
        for mode in 0..4 {
            let mut memory = Memory::new();
            memory.drift = mode == 0;
            memory.identity_drift = mode == 1;
            memory.deny_capacity = mode == 2;
            memory.deny_path = mode == 3;
            assert!(build(&memory, &[write("src/a", b"after")]).is_err());
            assert_eq!(memory.files["src/a"].bytes, b"uncommitted a");
        }
    }
    #[test]
    fn supplemental_files_still_require_authority_and_order_against_other_tasks() {
        let memory = Memory::new();
        let task = task();
        let mut collection = plan(&task);
        collection["tasks"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"TASK-002","dependencies":[],
            "files":[{"action":"modify","path":"src/c"}]}));
        assert!(
            prepare(
                &memory,
                &collection,
                &task,
                &authority(),
                &[write("src/c", b"after")],
                |_| Ok(metadata())
            )
            .is_err()
        );
        collection["tasks"][1]["dependencies"] = json!(["TASK-001"]);
        assert!(
            prepare(
                &memory,
                &collection,
                &task,
                &authority(),
                &[write("src/c", b"after")],
                |_| Ok(metadata())
            )
            .is_ok()
        );
        let mut limited = authority();
        limited["authorization"]["modifiable_files"] = json!(["src/a"]);
        let denied = prepare(
            &memory,
            &plan(&task),
            &task,
            &limited,
            &[write("src/c", b"after")],
            |_| Ok(metadata()),
        );
        assert_eq!(
            denied.err().unwrap().reason_code,
            "execution_authorization_scope_expansion"
        );
        let unsupported = prepare(
            &memory,
            &plan(&task),
            &task,
            &authority(),
            &[write("src/c", b"after")],
            |_| Err(error("file_transaction_metadata_unsupported")),
        );
        assert!(unsupported.is_err());
    }
    #[test]
    fn retained_supplements_of_other_tasks_require_explicit_order() {
        let memory = Memory::new();
        let task = task();
        let collection = json!({"tasks":[task, {"id":"TASK-002","dependencies":[],
            "files":[{"action":"modify","path":"unrelated"}]}]});
        let scopes = BTreeMap::from([("TASK-002".into(), vec!["src/c".into()])]);
        let mut effective = effective_collection(&collection, &scopes).unwrap();
        let result = prepare(
            &memory,
            &effective,
            &task,
            &authority(),
            &[write("src/c", b"second")],
            |_| Ok(metadata()),
        );
        assert!(result.is_err());
        assert!(!memory.files.contains_key("src/c"));
        effective["tasks"][0]["dependencies"] = json!(["TASK-002"]);
        assert!(
            prepare(
                &memory,
                &effective,
                &effective["tasks"][0],
                &authority(),
                &[write("src/c", b"second")],
                |_| Ok(metadata())
            )
            .is_ok()
        );
        assert_eq!(collection["tasks"][0]["dependencies"], json!([]));
    }
    #[test]
    fn moving_changed_content_requires_both_move_and_modify_authorization() {
        let memory = Memory::new();
        let task = task();
        let mut authorization = authority();
        authorization["authorization"]["scope"] = json!({
            "project_root":"/project","requirement_id":"example","goal":"Approved change",
            "module_paths":["src"],"modification_types":["move"],
            "risk_boundary":"Local changes only","validation_scope":["tests"],
            "task_ids":["TASK-001"],"adjustments":[],"revoked_reason":null
        });
        for bytes in [None, Some(b"uncommitted a".as_slice())] {
            assert!(
                prepare(
                    &memory,
                    &plan(&task),
                    &task,
                    &authorization,
                    &[mv("src/a", "src/c", bytes)],
                    |_| unreachable!()
                )
                .is_ok()
            );
        }
        let changed = [mv("src/a", "src/c", Some(b"edited move"))];
        assert_eq!(
            prepare(
                &memory,
                &plan(&task),
                &task,
                &authorization,
                &changed,
                |_| unreachable!()
            )
            .err()
            .unwrap()
            .reason_code,
            "execution_authorization_scope_expansion"
        );
        authorization["authorization"]["scope"]["modification_types"] = json!(["move", "modify"]);
        assert!(
            prepare(
                &memory,
                &plan(&task),
                &task,
                &authorization,
                &changed,
                |_| unreachable!()
            )
            .is_ok()
        );
        assert_eq!(memory.files["src/a"].bytes, b"uncommitted a");
    }
    #[test]
    fn current_scope_modification_types_exact_files_and_revocation_remain_binding() {
        for mode in 0..3 {
            let memory = Memory::new();
            let task = task();
            let mut authorization = authority();
            authorization["authorization"]["scope"] = json!({
                "project_root":"/project","requirement_id":"example","goal":"Approved change",
                "module_paths":["src"],"modification_types":["modify"],
                "risk_boundary":"Local changes only","validation_scope":["tests"],
                "task_ids":["TASK-001"],"adjustments":["additional_files"],"revoked_reason":null
            });
            if mode == 1 {
                authorization["authorization"]["scope"]["exact_files"] = json!(["src/a"]);
            }
            if mode == 2 {
                authorization["authorization"]["scope"]["revoked_reason"] =
                    json!("User withdrew approval");
            }
            let metadata_called = Cell::new(false);
            let result = prepare(
                &memory,
                &plan(&task),
                &task,
                &authorization,
                &[write("src/c", b"new")],
                |_| {
                    metadata_called.set(true);
                    Ok(metadata())
                },
            );
            assert!(result.is_err());
            assert!(
                !metadata_called.get(),
                "Unauthorized creates must be rejected before private metadata preparation"
            );
            assert_eq!(memory.files.len(), 2);
        }
    }
}
