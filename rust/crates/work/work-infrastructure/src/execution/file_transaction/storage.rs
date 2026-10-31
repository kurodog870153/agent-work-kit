//! Durable project file transactions using the existing requirement lock and runtime manifest.
use super::resources;
use crate::execution::storage::LocalExecutionStorage;
use crate::files::{LocalFiles, require_runtime_same_filesystem, resolve_runtime_path};
use crate::transaction_storage::{
    prepare_runtime_transaction, runtime_manifest_directory, update_runtime_progress,
};
use crate::writer_lock::LocalWriterLock;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use work_feature::error::WorkError;
use work_feature::execution::ExecutionWriterContext;
use work_feature::execution::RuntimeExecutionReadiness;
use work_feature::execution::file_transaction::{FileTransactionRepository, error, prepare};
use work_feature::ports::{ArtifactStore, RuntimeWriterLock, with_runtime_writer};
use work_model::execution::file_transaction::*;
use work_model::execution::host_file_report::{HostFileEvidence, HostFileReport, HostFileStatus};
use work_model::runtime::{LockClass, RuntimeManifest, RuntimePhase};
use work_model::schema::PublicSchema;
use work_operations::derivation::{file_transaction as derive, fingerprint, publication};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalEvidence {
    transaction_identity: String,
    phase: RuntimePhase,
    path_identities: BTreeMap<String, Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    host_report: Option<HostFileReport>,
}
fn terminal_prefix(phase: RuntimePhase) -> Result<&'static str, WorkError> {
    match phase {
        RuntimePhase::PublishedVerified => Ok("publication"),
        RuntimePhase::Restored => Ok("restoration"),
        _ => Err(error("file_transaction_terminal_evidence_invalid")),
    }
}

pub struct LocalFileTransactions {
    root: PathBuf,
    owned: bool,
    instruction_root: Option<PathBuf>,
}

fn temporary_index(name: &str, count: usize) -> Result<(usize, bool), WorkError> {
    let (direction, index) = name
        .strip_suffix(".tmp")
        .and_then(|s| s.rsplit_once('-'))
        .ok_or_else(|| error("file_transaction_inventory_foreign"))?;
    let index: usize = index
        .parse()
        .map_err(|_| error("file_transaction_inventory_foreign"))?;
    if index >= count || name != format!("{direction}-{index}.tmp") {
        return Err(error("file_transaction_inventory_foreign"));
    }
    let publish = match direction {
        "publish" => true,
        "restore" => false,
        #[cfg(any(windows, target_os = "macos"))]
        "restore-metadata" => false,
        _ => return Err(error("file_transaction_inventory_foreign")),
    };
    Ok((index, publish))
}
fn temporary_prefix(expected: Option<&[u8]>, actual: &[u8]) -> Result<(), WorkError> {
    if !expected.is_some_and(|bytes| bytes.starts_with(actual)) {
        return Err(error("file_transaction_temporary_corrupt"));
    }
    Ok(())
}
impl LocalFileTransactions {
    pub fn bind_host(
        &self,
        context: &ExecutionWriterContext,
        attempt_id: &str,
        changes: &[FileChange],
        baseline_confirmation: Option<&FileBaselineConfirmation>,
    ) -> Result<FileTransactionPreview, WorkError> {
        with_runtime_writer(
            &LocalWriterLock,
            context.writer(),
            LockClass::Execution,
            |_| {
                let owned = Self {
                    root: self.root.clone(),
                    owned: true,
                    instruction_root: self.instruction_root.clone(),
                };
                let preview = prepare(&owned, context, attempt_id, changes, baseline_confirmation)?;
                let binding = Self::binding(&preview.manifest)?;
                let attempt_path = format!(
                    "{}/{}/{}/attempt.json",
                    context.target().execution_dir,
                    binding.task_id,
                    binding.attempt_id
                );
                let attempt: serde_json::Value =
                    serde_json::from_slice(&self.read_source(&attempt_path)?)
                        .map_err(|_| error("file_transaction_attempt_invalid"))?;
                let authorization = attempt["authorization"]["authorization_evidence"]
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| error("file_transaction_authorization_required"))?;
                let manifest = &preview.manifest;
                let candidate = derive::candidate_bytes(&binding, &manifest.targets);
                self.check_sources(context, &binding, None)?;
                if self.observed(manifest)? != Self::expected(manifest, &binding, false)? {
                    return Err(error("file_transaction_target_drift"));
                }
                for (path, expected) in &binding.path_identities {
                    if self.path_identity(path)? != *expected {
                        return Err(error("file_transaction_source_drift"));
                    }
                }
                prepare_runtime_transaction(
                    &LocalFiles,
                    &self.root,
                    manifest,
                    &BTreeMap::from([("candidate.json".into(), candidate)]),
                )?;
                if self.load(&manifest.requirement_id, &manifest.transaction_identity)? != *manifest
                {
                    return Err(error("file_transaction_preparation_readback"));
                }
                self.authorize(manifest, authorization, None)?;
                Ok(preview)
            },
        )
    }

    pub fn record_host(
        &self,
        context: &ExecutionWriterContext,
        report: &HostFileReport,
    ) -> Result<FileTransactionResult, WorkError> {
        with_runtime_writer(
            &LocalWriterLock,
            context.writer(),
            LockClass::Execution,
            |_| {
                let manifest = self.load(
                    context.writer().requirement_id.as_str(),
                    &report.transaction_identity,
                )?;
                if manifest.phase != RuntimePhase::Prepared {
                    return Err(error("file_transaction_host_record_phase"));
                }
                self.validate_context(context, &manifest)?;
                self.validate_host_report(&manifest, report)?;
                let binding = Self::binding(&manifest)?;
                self.check_sources(context, &binding, Some(&manifest))?;
                let authorization_path = format!(
                    "{}/{}/{}/attempt.json",
                    context.target().execution_dir,
                    binding.task_id,
                    binding.attempt_id
                );
                let attempt: serde_json::Value =
                    serde_json::from_slice(&self.read_source(&authorization_path)?)
                        .map_err(|_| error("file_transaction_attempt_invalid"))?;
                let authorization = attempt["authorization"]["authorization_evidence"]
                    .as_str()
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| error("file_transaction_authorization_required"))?;
                let approval = FileAuthorization {
                    transaction_identity: manifest.transaction_identity.clone(),
                    publication_sha256: manifest.approval_sha256.clone(),
                    authorization_evidence: authorization.into(),
                    recovery: None,
                };
                let authorization_file = self
                    .directory(&manifest.requirement_id, &manifest.transaction_identity)?
                    .join(derive::authorization_path(&approval));
                if !authorization_file.is_file() {
                    return Err(error("file_transaction_authorization_missing"));
                }
                let expected = Self::expected(&manifest, &binding, true)?;
                if self.observed(&manifest)? != expected {
                    return Err(error("file_transaction_host_target_mismatch"));
                }
                let identities: BTreeMap<_, _> = expected
                    .keys()
                    .map(|path| Ok((path.clone(), self.path_identity(path)?)))
                    .collect::<Result<_, WorkError>>()?;
                if identities
                    .iter()
                    .any(|(path, identity)| identity.is_some() != expected[path].is_some())
                {
                    return Err(error("file_transaction_host_identity_mismatch"));
                }
                self.commit_terminal(
                    &manifest,
                    RuntimePhase::PublishedVerified,
                    &identities,
                    Some(report.clone()),
                )?;
                if self.observed(&manifest)? != expected {
                    return Err(error("file_transaction_host_target_mismatch"));
                }
                self.validate_host_report(&manifest, report)?;
                for (path, expected_identity) in &identities {
                    if self.path_identity(path)? != *expected_identity {
                        return Err(error("file_transaction_host_identity_mismatch"));
                    }
                }
                update_runtime_progress(
                    &LocalFiles,
                    &self.root,
                    &manifest,
                    manifest.targets.len(),
                    RuntimePhase::PublishedVerified,
                )?;
                Ok(FileTransactionResult {
                    schema: PublicSchema::WorkFileTransactionResult,
                    transaction_identity: manifest.transaction_identity,
                    phase: RuntimePhase::PublishedVerified,
                })
            },
        )
    }
    fn validate_host_report(
        &self,
        manifest: &RuntimeManifest,
        report: &HostFileReport,
    ) -> Result<(), WorkError> {
        let binding = Self::binding(manifest)?;
        if !report.valid_shape()
            || report.status != HostFileStatus::CompletedAtomic
            || report.task_id != binding.task_id
            || report.attempt_id != binding.attempt_id
            || report.transaction_identity != manifest.transaction_identity
            || report.approval_sha256 != manifest.approval_sha256
        {
            return Err(error("file_transaction_host_report_binding"));
        }
        let raw = self.read_source(&report.result_reference)?;
        if fingerprint::raw(&raw) != report.result_sha256 {
            return Err(error("file_transaction_host_evidence_drift"));
        }
        let evidence_value = work_operations::canonical::parse_json_contract(&raw)
            .map_err(|_| error("file_transaction_host_evidence_invalid"))?;
        let evidence: HostFileEvidence = serde_json::from_value(evidence_value)
            .map_err(|_| error("file_transaction_host_evidence_invalid"))?;
        if !evidence.valid_shape()
            || evidence.status != HostFileStatus::CompletedAtomic
            || evidence.task_id != report.task_id
            || evidence.attempt_id != report.attempt_id
            || evidence.transaction_identity != report.transaction_identity
            || evidence.paths.len() != manifest.targets.len()
            || manifest.targets.iter().any(|target| {
                evidence.paths.get(&target.path).is_none_or(|path| {
                    path.before_sha256 != target.before.as_ref().map(|state| state.sha256.clone())
                        || path.after_sha256
                            != target.after.as_ref().map(|state| state.sha256.clone())
                })
            })
        {
            return Err(error("file_transaction_host_evidence_invalid"));
        }
        Ok(())
    }
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            owned: false,
            instruction_root: None,
        }
    }
    pub fn for_project(root: PathBuf, instruction_root: PathBuf) -> Self {
        Self {
            root,
            owned: false,
            instruction_root: Some(instruction_root),
        }
    }
    fn path(&self, path: &str) -> Result<PathBuf, WorkError> {
        let file = resolve_runtime_path(&self.root, path)?;
        require_runtime_same_filesystem(&self.root, &file)?;
        Ok(file)
    }
    fn binding(manifest: &RuntimeManifest) -> Result<FileTransactionBinding, WorkError> {
        if manifest.business_identity.get("staged_files").is_some() {
            return Err(error("file_transaction_migration_required"));
        }
        serde_json::from_value(manifest.business_identity.clone())
            .map_err(|_| error("file_transaction_binding_invalid"))
    }
    fn validate_context(
        &self,
        context: &ExecutionWriterContext,
        manifest: &RuntimeManifest,
    ) -> Result<(), WorkError> {
        let binding = Self::binding(manifest)?;
        if manifest.canonical_root != context.writer().canonical_project_root.to_string_lossy()
            || manifest.requirement_id != context.writer().requirement_id.as_str()
            || manifest.execution_dir != context.target().execution_dir
            || binding.task_id != context.target().task_id
        {
            return Err(error("file_transaction_context_mismatch"));
        }
        let raw = self.read_source(&format!("{}/index.json", context.target().execution_dir))?;
        let index = work_operations::canonical::parse_json_contract(&raw)
            .map_err(|_| error("file_transaction_index_invalid"))?;
        if index["lock"]["task_id"] != binding.task_id
            || index["lock"]["attempt_id"] != binding.attempt_id
        {
            return Err(error("file_transaction_attempt_mismatch"));
        }
        Ok(())
    }
    fn expected(
        manifest: &RuntimeManifest,
        binding: &FileTransactionBinding,
        after: bool,
    ) -> Result<BTreeMap<String, Option<FileState>>, WorkError> {
        manifest
            .targets
            .iter()
            .map(|t| {
                let bytes = if after { &t.after } else { &t.before };
                let metadata = if after {
                    &binding.after_metadata
                } else {
                    &binding.before_metadata
                };
                let state = match (bytes, metadata.get(&t.path)) {
                    (Some(b), Some(Some(m))) => {
                        #[cfg(target_os = "macos")]
                        crate::execution::project_file_metadata::macos::ownership(m)?;
                        Some(FileState {
                            bytes: b.bytes.clone(),
                            metadata: m.clone(),
                        })
                    }
                    (None, Some(None)) => None,
                    _ => return Err(error("file_transaction_metadata_inventory")),
                };
                Ok((t.path.clone(), state))
            })
            .collect()
    }
    fn observed(
        &self,
        manifest: &RuntimeManifest,
    ) -> Result<BTreeMap<String, Option<FileState>>, WorkError> {
        self.check_resources(
            &manifest
                .targets
                .iter()
                .map(|t| t.path.clone())
                .collect::<Vec<_>>(),
        )?;
        manifest
            .targets
            .iter()
            .map(|t| Ok((t.path.clone(), self.inspect(&t.path)?)))
            .collect()
    }
    fn directory(&self, requirement: &str, identity: &str) -> Result<PathBuf, WorkError> {
        let requirement = requirement
            .parse()
            .map_err(|_| error("file_transaction_requirement"))?;
        let path = publication::runtime_staging_path(
            &requirement,
            publication::RuntimeOperation::ProjectFiles,
            identity,
        )
        .map_err(|_| error("file_transaction_identity"))?;
        self.path(&path)
    }
    /// Exact inventory and frozen candidate verification, including interrupted temporary writes.
    fn load(&self, requirement: &str, identity: &str) -> Result<RuntimeManifest, WorkError> {
        let dir = self.directory(requirement, identity)?;
        let raw = resources::read(&dir.join("transaction.json"), resources::MAX_CONTROL_BYTES)?;
        self.load_bytes(requirement, identity, &raw)
    }
    fn load_bytes(
        &self,
        requirement: &str,
        identity: &str,
        raw: &[u8],
    ) -> Result<RuntimeManifest, WorkError> {
        let dir = self.directory(requirement, identity)?;
        let manifest: RuntimeManifest =
            serde_json::from_slice(raw).map_err(|_| error("file_transaction_manifest_invalid"))?;
        // Recovery may observe small after files while restoring much larger before
        // files. Budget the frozen set before cloning, serializing or writing it.
        let mut size = 0_u64;
        for target in &manifest.targets {
            size = size
                .checked_add(target.path.len() as u64 + 4096)
                .ok_or_else(|| error("file_transaction_resource_limit"))?;
            for state in [&target.before, &target.after].into_iter().flatten() {
                size = size
                    .checked_add(state.bytes.len() as u64)
                    .ok_or_else(|| error("file_transaction_resource_limit"))?;
            }
            if target.after.is_some() {
                size = size
                    .checked_add(target.path.len() as u64 + 4096)
                    .ok_or_else(|| error("file_transaction_resource_limit"))?;
            }
        }
        let available = fs4::available_space(&self.root)
            .map_err(|_| error("file_transaction_storage_unverifiable"))?;
        resources::verify(size, available)?;
        if serde_json::to_vec(&manifest).expect("manifest serializes") != raw
            || manifest.operation != "project-files"
            || manifest.transaction_identity != identity
            || manifest.requirement_id != requirement
            || runtime_manifest_directory(&self.root, &manifest)? != dir
            || derive::approval(&manifest) != manifest.approval_sha256
        {
            return Err(error("file_transaction_manifest_binding"));
        }
        let binding = Self::binding(&manifest)?;
        for metadata in binding
            .before_metadata
            .values()
            .chain(binding.after_metadata.values())
            .flatten()
        {
            size = size
                .checked_add(
                    serde_json::to_vec(metadata)
                        .map_err(|_| error("file_transaction_metadata_read"))?
                        .len() as u64,
                )
                .ok_or_else(|| error("file_transaction_resource_limit"))?;
        }
        resources::verify(size, available)?;
        let frozen = derive::candidate_bytes(&binding, &manifest.targets);
        let payload = dir.join("candidate.json");
        // No target was touched until this full payload was written. A failed prepare retains
        // the manifest's complete before/after evidence and may be explicitly restored.
        if payload.exists() {
            let current = resources::read(&payload, resources::MAX_CONTROL_BYTES)?;
            if !frozen.starts_with(&current) {
                return Err(error("file_transaction_inventory_corrupt"));
            }
            if current != frozen && manifest.phase != RuntimePhase::Prepared {
                return Err(error("file_transaction_inventory_incomplete"));
            }
        } else if manifest.phase != RuntimePhase::Prepared {
            return Err(error("file_transaction_inventory_incomplete"));
        }
        if manifest.inventory.len() != 1
            || manifest.inventory[0].path != "candidate.json"
            || manifest.inventory[0].sha256 != fingerprint::raw(&frozen)
            || manifest.inventory[0].size_bytes != frozen.len() as u64
        {
            return Err(error("file_transaction_inventory_binding"));
        }
        let paths: std::collections::BTreeSet<_> =
            manifest.targets.iter().map(|t| &t.path).collect();
        if !binding
            .before_metadata
            .keys()
            .collect::<std::collections::BTreeSet<_>>()
            .eq(&paths)
            || !binding
                .after_metadata
                .keys()
                .collect::<std::collections::BTreeSet<_>>()
                .eq(&paths)
            || !binding
                .path_identities
                .keys()
                .collect::<std::collections::BTreeSet<_>>()
                .eq(&paths)
            || manifest
                .targets
                .iter()
                .any(|t| binding.path_identities[&t.path].is_some() != t.before.is_some())
        {
            return Err(error("file_transaction_metadata_inventory"));
        }
        Self::expected(&manifest, &binding, false)?;
        Self::expected(&manifest, &binding, true)?;
        let mut publish_authorized = false;
        let mut restore_authorized = false;
        for entry in fs::read_dir(&dir).map_err(|_| error("file_transaction_inventory_read"))? {
            let entry = entry.map_err(|_| error("file_transaction_inventory_read"))?;
            let name = entry
                .file_name()
                .to_str()
                .ok_or_else(|| error("file_transaction_inventory_foreign"))?
                .to_owned();
            let relative = entry
                .path()
                .strip_prefix(&self.root)
                .map_err(|_| error("file_transaction_root"))?
                .to_string_lossy()
                .replace('\\', "/");
            let path = self.path(&relative)?;
            if !path.is_file() {
                return Err(error("file_transaction_inventory_foreign"));
            }
            if matches!(name.as_str(), "transaction.json" | "candidate.json") {
                continue;
            }
            if name.starts_with("publication-") || name.starts_with("restoration-") {
                self.terminal_record(
                    &manifest,
                    &name,
                    &resources::read(&path, resources::MAX_CONTROL_BYTES)?,
                )?;
                continue;
            }
            if name.starts_with("authorization-") && name.ends_with(".json.tmp") {
                // An uncommitted authorization grants no authority. Retain it for exact
                // request revalidation; recovery may use a newly approved authorization.
                let final_name = name.strip_suffix(".tmp").expect("checked suffix");
                self.check_authorization_prefix(
                    &manifest,
                    final_name,
                    &resources::read(&path, resources::MAX_CONTROL_BYTES)?,
                )?;
                continue;
            }
            if name.starts_with("authorization-") && name.ends_with(".json") {
                let raw = resources::read(&path, resources::MAX_CONTROL_BYTES)?;
                let authorization: FileAuthorization = match serde_json::from_slice(&raw) {
                    Ok(value) => value,
                    Err(_) => {
                        // Older writers could leave a truncated final record. Do not
                        // overwrite it or count it as approval; require fresh evidence.
                        self.check_authorization_prefix(&manifest, &name, &raw)?;
                        continue;
                    }
                };
                if derive::authorization_path(&authorization) != name
                    || authorization.transaction_identity != manifest.transaction_identity
                    || authorization.publication_sha256 != manifest.approval_sha256
                    || authorization.authorization_evidence.trim().is_empty()
                {
                    return Err(error("file_transaction_authorization_binding"));
                }
                if let Some(recovery) = &authorization.recovery {
                    let mut frozen = manifest.clone();
                    frozen.phase = recovery.manifest.phase;
                    frozen.published_count = recovery.manifest.published_count;
                    if frozen != recovery.manifest
                        || recovery.approval_sha256
                            != derive::recovery_approval(
                                &recovery.manifest,
                                &recovery.observed,
                                &recovery.observed_identities,
                            )
                    {
                        return Err(error("file_transaction_authorization_binding"));
                    }
                    restore_authorized = true;
                } else {
                    publish_authorized = true;
                }
                continue;
            }
            if name == "transaction.json.tmp" {
                self.control_candidate(
                    &manifest,
                    &resources::read(&path, resources::MAX_CONTROL_BYTES)?,
                )?;
                continue;
            }
            let (n, publish) = temporary_index(&name, manifest.targets.len())?;
            let target = &manifest.targets[n];
            let bytes = if publish {
                &target.after
            } else {
                &target.before
            };
            let raw = resources::read(&path, resources::MAX_CONTROL_BYTES)?;
            temporary_prefix(bytes.as_ref().map(|b| b.bytes.as_slice()), &raw)?;
        }
        if (matches!(
            manifest.phase,
            RuntimePhase::Publishing | RuntimePhase::PublishedVerified
        ) && !publish_authorized)
            || (matches!(
                manifest.phase,
                RuntimePhase::Restoring | RuntimePhase::Restored
            ) && !restore_authorized)
        {
            return Err(error("file_transaction_authorization_missing"));
        }
        if matches!(
            manifest.phase,
            RuntimePhase::PublishedVerified | RuntimePhase::Restored
        ) {
            self.terminal_identities(&manifest)?;
        }
        Ok(manifest)
    }
    fn control_candidate(
        &self,
        manifest: &RuntimeManifest,
        raw: &[u8],
    ) -> Result<RuntimeManifest, WorkError> {
        for phase in [
            RuntimePhase::Publishing,
            RuntimePhase::PublishedVerified,
            RuntimePhase::Restoring,
            RuntimePhase::Restored,
        ] {
            if derive::phase_rank(phase) < derive::phase_rank(manifest.phase) {
                continue;
            }
            for count in manifest.published_count..=manifest.targets.len() {
                let mut next = manifest.clone();
                next.phase = phase;
                next.published_count = count;
                if next.validate_shape().is_ok()
                    && serde_json::to_vec(&next)
                        .expect("manifest serializes")
                        .starts_with(raw)
                {
                    return Ok(next);
                }
            }
        }
        Err(error("file_transaction_control_corrupt"))
    }
    fn terminal_record(
        &self,
        manifest: &RuntimeManifest,
        name: &str,
        raw: &[u8],
    ) -> Result<Option<TerminalEvidence>, WorkError> {
        let final_name = name.strip_suffix(".tmp").unwrap_or(name);
        let (prefix, digest) = final_name
            .strip_suffix(".json")
            .and_then(|n| n.split_once('-'))
            .ok_or_else(|| error("file_transaction_terminal_evidence_invalid"))?;
        let phase = match prefix {
            "publication" => RuntimePhase::PublishedVerified,
            "restoration" => RuntimePhase::Restored,
            _ => return Err(error("file_transaction_terminal_evidence_invalid")),
        };
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(error("file_transaction_terminal_evidence_invalid"));
        }
        if name.ends_with(".tmp") {
            let header = format!(
                "{{\"transaction_identity\":{},\"phase\":{},\"path_identities\":",
                serde_json::to_string(&manifest.transaction_identity).expect("identity serializes"),
                serde_json::to_string(&phase).expect("phase serializes")
            );
            let compared = raw.len().min(header.len());
            if raw[..compared] != header.as_bytes()[..compared] {
                return Err(error("file_transaction_terminal_evidence_invalid"));
            }
            return Ok(None);
        }
        let receipt: TerminalEvidence = serde_json::from_slice(raw)
            .map_err(|_| error("file_transaction_terminal_evidence_invalid"))?;
        let binding = Self::binding(manifest)?;
        let expected =
            Self::expected(manifest, &binding, phase == RuntimePhase::PublishedVerified)?;
        if receipt.transaction_identity != manifest.transaction_identity
            || receipt.phase != phase
            || serde_json::to_vec(&receipt).expect("receipt serializes") != raw
            || fingerprint::raw(raw) != digest
            || !receipt.path_identities.keys().eq(expected.keys())
            || expected.iter().any(|(p, s)| {
                receipt.path_identities[p].is_some() != s.is_some()
                    || receipt.path_identities[p]
                        .as_ref()
                        .is_some_and(|id| id.trim().is_empty())
            })
        {
            return Err(error("file_transaction_terminal_evidence_invalid"));
        }
        if let Some(report) = &receipt.host_report {
            if phase != RuntimePhase::PublishedVerified {
                return Err(error("file_transaction_terminal_evidence_invalid"));
            }
            self.validate_host_report(manifest, report)?;
        }
        Ok(Some(receipt))
    }
    fn terminal_identities(
        &self,
        manifest: &RuntimeManifest,
    ) -> Result<BTreeMap<String, Option<String>>, WorkError> {
        let dir = runtime_manifest_directory(&self.root, manifest)?;
        let prefix = format!("{}-", terminal_prefix(manifest.phase)?);
        let mut result = None;
        for entry in fs::read_dir(&dir).map_err(|_| error("file_transaction_inventory_read"))? {
            let entry = entry.map_err(|_| error("file_transaction_inventory_read"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) && name.ends_with(".json") {
                let receipt = self
                    .terminal_record(
                        manifest,
                        &name,
                        &resources::read(
                            &self.path(
                                &entry
                                    .path()
                                    .strip_prefix(&self.root)
                                    .map_err(|_| error("file_transaction_root"))?
                                    .to_string_lossy()
                                    .replace('\\', "/"),
                            )?,
                            resources::MAX_CONTROL_BYTES,
                        )?,
                    )?
                    .ok_or_else(|| error("file_transaction_terminal_evidence_invalid"))?;
                if result.replace(receipt.path_identities).is_some() {
                    return Err(error("file_transaction_terminal_evidence_duplicate"));
                }
            }
        }
        result.ok_or_else(|| error("file_transaction_terminal_evidence_missing"))
    }
    fn commit_terminal(
        &self,
        manifest: &RuntimeManifest,
        phase: RuntimePhase,
        identities: &BTreeMap<String, Option<String>>,
        host_report: Option<HostFileReport>,
    ) -> Result<(), WorkError> {
        let receipt = TerminalEvidence {
            transaction_identity: manifest.transaction_identity.clone(),
            phase,
            path_identities: identities.clone(),
            host_report,
        };
        let raw = serde_json::to_vec(&receipt).expect("receipt serializes");
        let dir = runtime_manifest_directory(&self.root, manifest)?;
        let name = format!(
            "{}-{}.json",
            terminal_prefix(phase)?,
            fingerprint::raw(&raw)
        );
        let path = dir.join(&name);
        if path.exists() {
            if resources::read(&path, resources::MAX_CONTROL_BYTES)? != raw {
                return Err(error("file_transaction_terminal_evidence_invalid"));
            }
        } else {
            let temporary = dir.join(format!("{name}.tmp"));
            if temporary.exists() {
                let prefix = resources::read(&temporary, resources::MAX_CONTROL_BYTES)?;
                if !raw.starts_with(&prefix) {
                    return Err(error("file_transaction_terminal_evidence_invalid"));
                }
                let mut file = OpenOptions::new()
                    .append(true)
                    .open(&temporary)
                    .map_err(|_| error("file_transaction_terminal_evidence_write"))?;
                file.write_all(&raw[prefix.len()..])
                    .and_then(|_| file.sync_all())
                    .map_err(|_| error("file_transaction_terminal_evidence_write"))?;
            } else {
                LocalFiles.create_new(&temporary, &raw)?;
            }
            if resources::read(&temporary, resources::MAX_CONTROL_BYTES)? != raw {
                return Err(error("file_transaction_terminal_evidence_invalid"));
            }
            crate::execution::control_commit::new(&temporary, &path)?;
        }
        if resources::read(&path, resources::MAX_CONTROL_BYTES)? != raw {
            return Err(error("file_transaction_terminal_evidence_invalid"));
        }
        Ok(())
    }
    fn check_sources(
        &self,
        context: &ExecutionWriterContext,
        binding: &FileTransactionBinding,
        publishing: Option<&RuntimeManifest>,
    ) -> Result<(), WorkError> {
        for path in &binding.directory_inputs {
            if !self.is_directory(path)? {
                return Err(error("file_transaction_source_drift"));
            }
        }
        for (p, h) in &binding.source_sha256 {
            if publishing
                .is_some_and(|manifest| manifest.targets.iter().any(|target| target.path == *p))
            {
                continue;
            }
            if fingerprint::raw(&self.read_source(p)?) != *h {
                return Err(error("file_transaction_source_drift"));
            }
        }
        let task = context.task_context().contract["tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| t["id"] == binding.task_id)
            .ok_or_else(|| error("file_transaction_task_missing"))?;
        let attempt: serde_json::Value = serde_json::from_slice(&self.read_source(&format!(
            "{}/{}/{}/attempt.json",
            context.target().execution_dir,
            binding.task_id,
            binding.attempt_id
        ))?)
        .map_err(|_| error("file_transaction_attempt_invalid"))?;
        work_feature::execution::file_transaction::verify_execute(self, task, &attempt)?;
        Ok(())
    }
    fn authorize(
        &self,
        manifest: &RuntimeManifest,
        evidence: &str,
        recovery: Option<FileRecoveryPreview>,
    ) -> Result<(), WorkError> {
        let dir = runtime_manifest_directory(&self.root, manifest)?;
        let authorization = FileAuthorization {
            transaction_identity: manifest.transaction_identity.clone(),
            publication_sha256: manifest.approval_sha256.clone(),
            authorization_evidence: evidence.into(),
            recovery,
        };
        let path = dir.join(derive::authorization_path(&authorization));
        let bytes = serde_json::to_vec(&authorization).expect("authorization serializes");
        if path.exists() {
            if resources::read(&path, resources::MAX_CONTROL_BYTES)? != bytes {
                return Err(error("file_transaction_authorization_corrupt"));
            }
        } else {
            let temporary = path.with_file_name(format!(
                "{}.tmp",
                derive::authorization_path(&authorization)
            ));
            if temporary.exists() {
                let partial = resources::read(&temporary, resources::MAX_CONTROL_BYTES)?;
                if !bytes.starts_with(&partial) {
                    return Err(error("file_transaction_authorization_corrupt"));
                }
                let mut file = OpenOptions::new()
                    .append(true)
                    .open(&temporary)
                    .map_err(|_| error("file_transaction_authorization_write"))?;
                file.write_all(&bytes[partial.len()..])
                    .and_then(|_| file.sync_all())
                    .map_err(|_| error("file_transaction_authorization_write"))?;
            } else {
                LocalFiles.create_new(&temporary, &bytes)?;
            }
            if resources::read(&temporary, resources::MAX_CONTROL_BYTES)? != bytes || path.exists()
            {
                return Err(error("file_transaction_authorization_corrupt"));
            }
            crate::execution::control_commit::new(&temporary, &path)?;
        }
        if resources::read(&path, resources::MAX_CONTROL_BYTES)? != bytes {
            return Err(error("file_transaction_authorization_corrupt"));
        }
        Ok(())
    }
    fn check_authorization_prefix(
        &self,
        manifest: &RuntimeManifest,
        name: &str,
        raw: &[u8],
    ) -> Result<(), WorkError> {
        let hash = name
            .strip_prefix("authorization-")
            .and_then(|s| s.strip_suffix(".json"))
            .filter(|s| {
                s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            })
            .ok_or_else(|| error("file_transaction_authorization_corrupt"))?;
        let _ = hash;
        let header = format!(
            "{{\"transaction_identity\":{},\"publication_sha256\":{},\"authorization_evidence\":",
            serde_json::to_string(&manifest.transaction_identity).expect("identity serializes"),
            serde_json::to_string(&manifest.approval_sha256).expect("approval serializes")
        );
        let compared = raw.len().min(header.len());
        if raw[..compared] != header.as_bytes()[..compared] {
            return Err(error("file_transaction_authorization_corrupt"));
        }
        Ok(())
    }
}

impl FileTransactionRepository for LocalFileTransactions {
    fn input_paths(
        &self,
        context: &ExecutionWriterContext,
        task: &serde_json::Value,
        attempt: &str,
    ) -> Result<Vec<String>, WorkError> {
        resolved_input_paths(context, task, attempt)
    }
    fn migration_review_sources(
        &self,
        context: &ExecutionWriterContext,
    ) -> Result<BTreeMap<String, Vec<u8>>, WorkError> {
        crate::specification::migration::verification::task_review_sources(context)
    }
    fn is_directory(&self, path: &str) -> Result<bool, WorkError> {
        let metadata = fs::symlink_metadata(self.path(path)?)
            .map_err(|_| error("file_transaction_input_unverified"))?;
        Ok(metadata.is_dir())
    }
    fn accepted_file_scopes(
        &self,
        context: &ExecutionWriterContext,
    ) -> Result<BTreeMap<String, Vec<String>>, WorkError> {
        let mut scopes = BTreeMap::<String, Vec<String>>::new();
        let storage = LocalExecutionStorage {
            project_root: self.root.clone(),
        };
        for task in context.task_context().collection["tasks"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = task["id"]
                .as_str()
                .ok_or_else(|| error("file_transaction_task_missing"))?;
            for name in work_feature::execution::AttemptStartRepository::attempt_names(
                &storage,
                context.target().execution_dir,
                id,
            )? {
                let path = format!(
                    "{}/{id}/{name}/attempt.json",
                    context.target().execution_dir
                );
                let raw = self.read_source(&path)?;
                let saved = work_operations::canonical::parse_json_contract(&raw)
                    .map_err(|_| error("file_transaction_source_invalid"))?;
                work_operations::execution::attempt::validate_attempt_bytes(&saved, &raw)
                    .map_err(|e| error(e.reason_code))?;
                for deviation in saved["execution_deviations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    if deviation["decision"]["outcome"] == "approved"
                        && work_operations::execution::deviation_reconciliation_target(
                            &deviation["proposal"],
                        ) == "task_only"
                    {
                        scopes.entry(id.into()).or_default().extend(
                            deviation["supplemental_authorization"]["modifiable_files"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|p| p.as_str().map(str::to_owned)),
                        );
                    }
                }
            }
        }
        for manifest in check_retained(
            &self.root,
            context.writer().requirement_id.as_str(),
            context.target().execution_dir,
        )? {
            let binding = Self::binding(&manifest)?;
            // Restored attempts also accepted this scope; recovery does not erase ordering obligations.
            let paths = scopes.entry(binding.task_id).or_default();
            for target in manifest.targets {
                if !paths.contains(&target.path) {
                    paths.push(target.path);
                }
            }
        }
        Ok(scopes)
    }
    fn history(
        &self,
        context: &ExecutionWriterContext,
        attempt_id: &str,
    ) -> Result<work_operations::execution::file_transaction::history::Current, WorkError> {
        retained_history(context, attempt_id)
    }
    fn new_file_metadata(&self, path: &str) -> Result<FileMetadata, WorkError> {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let target = self.path(path)?;
        let parent = target
            .parent()
            .filter(|p| p.is_dir())
            .ok_or_else(|| error("file_transaction_parent_missing"))?;
        let name = format!(
            ".work-file-metadata-{}-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|_| error("file_transaction_metadata_unavailable"))?
                .as_nanos(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let probe = parent.join(name);
        let relative = probe
            .strip_prefix(&self.root)
            .map_err(|_| error("file_transaction_root"))?
            .to_string_lossy()
            .replace('\\', "/");
        LocalFiles.create_new(&probe, &[])?;
        let identity = self.path_identity(&relative)?;
        let measured = self.inspect(&relative);
        // Only this exact still-empty file created by this invocation is disposable.
        if self.path_identity(&relative)? != identity
            || fs::metadata(&probe)
                .map_err(|_| error("file_transaction_metadata_read"))?
                .len()
                != 0
        {
            return Err(error("file_transaction_metadata_probe_changed"));
        }
        fs::remove_file(&probe).map_err(|_| error("file_transaction_metadata_probe_cleanup"))?;
        measured?
            .map(|s| s.metadata)
            .ok_or_else(|| error("file_transaction_metadata_unavailable"))
    }

    fn check_complete_resources(&self, weighted_bytes: u64) -> Result<(), WorkError> {
        let available = fs4::available_space(&self.root)
            .map_err(|_| error("file_transaction_storage_unverifiable"))?;
        resources::verify(weighted_bytes, available)
    }
    fn path_identity(&self, path: &str) -> Result<Option<String>, WorkError> {
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            let _ = path;
            Err(error("file_transaction_platform_unsupported"))
        }
        #[cfg(any(windows, target_os = "macos"))]
        {
            let path = self.path(path)?;
            let mut options = OpenOptions::new();
            options.read(true);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(0x00200000);
            }
            #[cfg(target_os = "macos")]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(0x20000000 | 0x01000000 | 0x4);
            }
            let file = match options.open(&path) {
                Ok(file) => file,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(_) => return Err(error("file_transaction_identity_unavailable")),
            };
            let metadata = file
                .metadata()
                .map_err(|_| error("file_transaction_identity_unavailable"))?;
            if !metadata.is_file() {
                return Err(error("file_transaction_target_unsupported"));
            }
            #[cfg(windows)]
            {
                let (attributes, links) =
                    crate::files::windows_runtime_metadata::descriptor_attributes_and_links(&file)
                        .map_err(|_| error("file_transaction_identity_unavailable"))?;
                if attributes & 0x400 != 0 || links != 1 {
                    return Err(error("file_transaction_path_alias"));
                }
                let id = crate::files::windows_runtime_metadata::descriptor_identity(&file)
                    .map_err(|_| error("file_transaction_identity_unavailable"))?;
                Ok(Some(format!(
                    "windows:{:08x}:{:08x}:{:08x}:{:08x}:{:08x}",
                    id[0], id[1], id[2], id[3], id[4]
                )))
            }
            #[cfg(target_os = "macos")]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.nlink() != 1 {
                    return Err(error("file_transaction_path_alias"));
                }
                Ok(Some(format!("macos:{}:{}", metadata.dev(), metadata.ino())))
            }
        }
    }

    fn check_resources(&self, paths: &[String]) -> Result<(), WorkError> {
        let mut size = 0_u64;
        for path in paths {
            let target = self.path(path)?;
            let length = match fs::metadata(&target) {
                Ok(metadata) => metadata.len(),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
                Err(_) => return Err(error("file_transaction_file_read")),
            };
            #[cfg(target_os = "macos")]
            if target.is_file() {
                let file =
                    fs::File::open(&target).map_err(|_| error("file_transaction_metadata_read"))?;
                for (name, value) in
                    crate::execution::project_file_metadata::macos::read_xattrs(&file)?
                {
                    size = size
                        .checked_add(name.len() as u64)
                        .and_then(|size| size.checked_add(value.len() as u64))
                        .ok_or_else(|| error("file_transaction_resource_limit"))?;
                }
            }
            size = size
                .checked_add(length)
                .and_then(|size| size.checked_add(path.len() as u64 + 4096))
                .ok_or_else(|| error("file_transaction_resource_limit"))?;
            resources::requirements(size)?;
        }
        let available = fs4::available_space(&self.root)
            .map_err(|_| error("file_transaction_storage_unverifiable"))?;
        resources::verify(size, available)
    }
    fn execute_selection(
        &self,
        task: &serde_json::Value,
        attempt: &serde_json::Value,
    ) -> Result<serde_json::Value, WorkError> {
        let root = self
            .instruction_root
            .as_ref()
            .ok_or_else(|| error("file_transaction_instruction_repository_required"))?;
        let selected: Vec<_> = task["instruction_selection"]["selected_paths"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p.as_str().map(str::to_owned))
            .collect();
        let mut references = vec!["execute.general.execution-records".to_owned()];
        if attempt.get("continued_from").is_some() {
            references.push("execute.general.execution-recovery".into());
        }
        Ok(serde_json::json!(work_feature::instruction::select(
            &crate::hierarchy_catalog::LocalHierarchyCatalog {
                skill_root: root.clone()
            },
            "execute",
            &selected,
            &references
        )?))
    }
    fn read_source(&self, path: &str) -> Result<Vec<u8>, WorkError> {
        resources::read(&self.path(path)?, resources::MAX_CONTROL_BYTES)
    }
    fn inspect(&self, path: &str) -> Result<Option<FileState>, WorkError> {
        if !cfg!(any(windows, target_os = "macos")) {
            return Err(error("file_transaction_platform_unsupported"));
        }
        let target = self.path(path)?;
        if !target.parent().is_some_and(Path::is_dir) {
            return Err(error("file_transaction_parent_missing"));
        }
        let file = match OpenOptions::new().read(true).open(&target) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(error("file_transaction_file_read")),
        };
        let metadata = file
            .metadata()
            .map_err(|_| error("file_transaction_metadata_read"))?;
        if !metadata.is_file() || metadata.permissions().readonly() {
            return Err(error("file_transaction_metadata_unsupported"));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & !(0x20 | 0x80) != 0 {
                return Err(error("file_transaction_metadata_unsupported"));
            }
        }
        #[cfg(unix)]
        let unix_mode = {
            use std::os::unix::fs::PermissionsExt;
            Some(metadata.permissions().mode())
        };
        #[cfg(not(unix))]
        let unix_mode = None;
        #[cfg(target_os = "macos")]
        let (unix_uid, unix_gid) = crate::execution::project_file_metadata::macos::read(&file)?;
        let permissions = FileMetadata {
            readonly: metadata.permissions().readonly(),
            unix_mode,
            #[cfg(target_os = "macos")]
            unix_uid: Some(unix_uid),
            #[cfg(not(target_os = "macos"))]
            unix_uid: None,
            #[cfg(target_os = "macos")]
            unix_gid: Some(unix_gid),
            #[cfg(not(target_os = "macos"))]
            unix_gid: None,
            #[cfg(windows)]
            windows_security: Some(crate::execution::project_file_metadata::windows::read(
                &target,
            )?),
            #[cfg(not(windows))]
            windows_security: None,
            #[cfg(target_os = "macos")]
            macos_xattrs: Some(crate::execution::project_file_metadata::macos::read_xattrs(
                &file,
            )?),
            #[cfg(not(target_os = "macos"))]
            macos_xattrs: None,
        };
        if permissions
            .windows_security
            .as_ref()
            .is_some_and(|security| security.len() > 4096)
        {
            return Err(error("file_transaction_metadata_resource_limit"));
        }
        let mut bytes = Vec::new();
        if metadata.len() > resources::MAX_PROJECT_BYTES {
            return Err(error("file_transaction_resource_limit"));
        }
        bytes
            .try_reserve_exact(metadata.len() as usize)
            .map_err(|_| error("file_transaction_memory_insufficient"))?;
        (&file)
            .take(resources::MAX_PROJECT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| error("file_transaction_file_read"))?;
        if target
            .metadata()
            .map_err(|_| error("file_transaction_metadata_read"))?
            .len()
            != bytes.len() as u64
            || bytes.len() as u64 > resources::MAX_PROJECT_BYTES
        {
            return Err(error("file_transaction_target_drift"));
        }
        #[cfg(target_os = "macos")]
        {
            use std::os::unix::fs::PermissionsExt;
            if crate::execution::project_file_metadata::macos::read(&file)? != (unix_uid, unix_gid)
                || Some(crate::execution::project_file_metadata::macos::read_xattrs(
                    &file,
                )?) != permissions.macos_xattrs
                || file
                    .metadata()
                    .map_err(|_| error("file_transaction_metadata_read"))?
                    .permissions()
                    .mode()
                    != unix_mode.unwrap()
            {
                return Err(error("file_transaction_target_drift"));
            }
        }
        Ok(Some(FileState {
            bytes,
            metadata: permissions,
        }))
    }
    fn check_prepare(&self, context: &ExecutionWriterContext) -> Result<(), WorkError> {
        if !self.owned {
            LocalWriterLock.require_runtime_idle(context.writer(), LockClass::Execution)?;
        }
        let storage = LocalExecutionStorage {
            project_root: self.root.clone(),
        };
        storage.check_command_context(context, false)?;
        Ok(())
    }
    fn recovery_preview(
        &self,
        context: &ExecutionWriterContext,
        identity: &str,
    ) -> Result<FileRecoveryPreview, WorkError> {
        if !self.owned {
            LocalWriterLock.require_runtime_idle(context.writer(), LockClass::Execution)?;
        }
        let manifest = self.load(context.writer().requirement_id.as_str(), identity)?;
        // Recovery owns only this transaction. Every unrelated pending allocation is a blocker.
        let namespace = format!(
            "outputs/work/runtime/staging/{}",
            context.writer().requirement_id.as_str()
        );
        for operation in fs::read_dir(self.path(&namespace)?)
            .map_err(|_| error("file_transaction_inventory_read"))?
        {
            let operation = operation.map_err(|_| error("file_transaction_inventory_read"))?;
            let relative = operation
                .path()
                .strip_prefix(&self.root)
                .map_err(|_| error("file_transaction_root"))?
                .to_string_lossy()
                .replace('\\', "/");
            let path = self.path(&relative)?;
            for entry in fs::read_dir(path).map_err(|_| error("file_transaction_inventory_read"))? {
                let entry = entry.map_err(|_| error("file_transaction_inventory_read"))?;
                let name = entry.file_name();
                if operation.file_name() != "project-files" {
                    return Err(error("file_transaction_foreign_pending"));
                }
                let other = self.load(
                    context.writer().requirement_id.as_str(),
                    name.to_str()
                        .ok_or_else(|| error("file_transaction_identity"))?,
                )?;
                if other.transaction_identity != identity
                    && !matches!(
                        other.phase,
                        RuntimePhase::PublishedVerified | RuntimePhase::Restored
                    )
                {
                    return Err(error("file_transaction_foreign_pending"));
                }
            }
        }
        self.validate_context(context, &manifest)?;
        if manifest.phase == RuntimePhase::PublishedVerified {
            return Err(error("file_transaction_already_published"));
        }
        let binding = Self::binding(&manifest)?;
        let before = Self::expected(&manifest, &binding, false)?;
        let after = Self::expected(&manifest, &binding, true)?;
        let observed_identities: BTreeMap<_, _> = manifest
            .targets
            .iter()
            .map(|t| Ok((t.path.clone(), self.path_identity(&t.path)?)))
            .collect::<Result<_, WorkError>>()?;
        let observed = self.observed(&manifest)?;
        for (p, s) in &observed {
            if *s != before[p] && *s != after[p] {
                return Err(error("file_transaction_external_change"));
            }
        }
        for (path, expected) in &observed_identities {
            if self.path_identity(path)? != *expected {
                return Err(error("file_transaction_external_change"));
            }
        }
        let approval_sha256 = derive::recovery_approval(&manifest, &observed, &observed_identities);
        Ok(FileRecoveryPreview {
            schema: PublicSchema::WorkFileRecoveryPreview,
            manifest,
            observed,
            observed_identities,
            approval_sha256,
        })
    }
}

/// Retained terminal evidence remains in runtime; pending or corrupt evidence blocks all writers.
pub fn check_retained(
    root: &Path,
    requirement: &str,
    execution: &str,
) -> Result<Vec<RuntimeManifest>, WorkError> {
    let store = LocalFileTransactions::new(root.to_path_buf());
    let namespace = format!("outputs/work/runtime/staging/{requirement}/project-files");
    let path = store.path(&namespace)?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let mut names = Vec::new();
    let mut budget = 0_u64;
    for entry in fs::read_dir(&path).map_err(|_| error("file_transaction_inventory_read"))? {
        let name = entry
            .map_err(|_| error("file_transaction_inventory_read"))?
            .file_name();
        let name = name
            .to_str()
            .ok_or_else(|| error("file_transaction_identity"))?
            .to_owned();
        let directory = store.directory(requirement, &name)?;
        let manifest = store.path(
            &directory
                .join("transaction.json")
                .strip_prefix(&store.root)
                .map_err(|_| error("file_transaction_root"))?
                .to_string_lossy()
                .replace('\\', "/"),
        )?;
        let length = fs::metadata(manifest)
            .map_err(|_| error("file_transaction_inventory_read"))?
            .len();
        if length > resources::MAX_CONTROL_BYTES {
            return Err(error("file_transaction_resource_limit"));
        }
        budget = budget
            .checked_add(length)
            .and_then(|n| n.checked_add(name.len() as u64 + 4096))
            .ok_or_else(|| error("file_transaction_memory_insufficient"))?;
        resources::history_memory(budget)?;
        names.push(name);
    }
    let mut result = Vec::new();
    for name in &names {
        let manifest = store.load(requirement, name)?;
        if manifest.execution_dir != execution {
            return Err(error("file_transaction_context_mismatch"));
        }
        if !matches!(
            manifest.phase,
            RuntimePhase::PublishedVerified | RuntimePhase::Restored
        ) {
            return Err(error("file_transaction_recovery_required"));
        }
        result.push(manifest);
    }
    Ok(result)
}

fn attempt_lineage(
    context: &ExecutionWriterContext,
    attempt_id: &str,
) -> Result<std::collections::BTreeSet<String>, WorkError> {
    let store = LocalFileTransactions::new(context.writer().canonical_project_root.clone());
    let mut lineage = std::collections::BTreeSet::new();
    let mut id = attempt_id.to_owned();
    let mut child: Option<serde_json::Value> = None;
    loop {
        if !lineage.insert(id.clone()) {
            return Err(error("file_transaction_continuation_invalid"));
        }
        let path = format!(
            "{}/{}/{id}/attempt.json",
            context.target().execution_dir,
            context.target().task_id
        );
        let raw = store.read_source(&path)?;
        let attempt = work_operations::canonical::parse_json_contract(&raw)
            .map_err(|_| error("file_transaction_continuation_invalid"))?;
        work_operations::execution::attempt::validate_attempt_bytes(&attempt, &raw)
            .map_err(|e| error(e.reason_code))?;
        if attempt["task_id"] != context.target().task_id || attempt["attempt_id"] != id {
            return Err(error("file_transaction_continuation_invalid"));
        }
        if let Some(child) = &child
            && (attempt["status"] != "stopped"
                || work_operations::execution::closed_task_status(
                    "stopped",
                    attempt["final_type"].as_str(),
                ) != "pending_retry"
                || [
                    "task_item_sha256",
                    "task_collection_sha256",
                    "task_index_sha256",
                    "task_instructions_sha256",
                ]
                .iter()
                .any(|key| attempt[*key] != child[*key]))
        {
            return Err(error("file_transaction_continuation_invalid"));
        }
        let Some(previous) = attempt["continued_from"].as_str() else {
            break;
        };
        if previous >= id.as_str() {
            return Err(error("file_transaction_continuation_invalid"));
        }
        id = previous.to_owned();
        child = Some(attempt);
    }
    Ok(lineage)
}

/// Follow only complete, authenticated publications, in predecessor order.
/// Each move in a publication maps from that publication's initial tree.
pub(crate) fn resolved_input_paths(
    context: &ExecutionWriterContext,
    task: &serde_json::Value,
    attempt_id: &str,
) -> Result<Vec<String>, WorkError> {
    let mut paths = work_operations::execution::file_transaction::input_paths(
        &context.task_context().contract,
        task,
    )
    .map_err(|e| error(e.reason_code))?;
    if paths.is_empty() {
        return Ok(paths);
    }
    let current = retained_history(context, attempt_id)?;
    let lineage = attempt_lineage(context, attempt_id)?;
    let mut transactions = BTreeMap::new();
    for manifest in check_retained(
        &context.writer().canonical_project_root,
        context.writer().requirement_id.as_str(),
        context.target().execution_dir,
    )? {
        let binding = LocalFileTransactions::binding(&manifest)?;
        if binding.task_id == context.target().task_id && lineage.contains(&binding.attempt_id) {
            transactions.insert(
                manifest.transaction_identity.clone(),
                (manifest.phase, binding),
            );
        }
    }
    let mut chain = Vec::new();
    let mut head = (!current.head.is_empty()).then_some(current.head.as_str());
    while let Some(id) = head {
        let row = transactions
            .get(id)
            .ok_or_else(|| error("file_transaction_history_disconnected"))?;
        chain.push(row);
        head = row.1.previous_transaction.as_deref();
    }
    for (phase, binding) in chain.into_iter().rev() {
        if *phase != RuntimePhase::PublishedVerified {
            continue;
        }
        for path in &mut paths {
            if let Some(destination) = binding.changes.iter().find_map(|change| match change {
                FileChange::Move {
                    source,
                    destination,
                    ..
                } if source == path => Some(destination),
                _ => None,
            }) {
                *path = destination.clone();
            }
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

pub(crate) fn continuation_baseline(
    context: &ExecutionWriterContext,
    attempt_id: &str,
) -> Result<BTreeMap<String, bool>, WorkError> {
    current_version(context, attempt_id)?;
    Ok(retained_history(context, attempt_id)?
        .initial
        .into_iter()
        .map(|(path, state)| (path, state.is_some()))
        .collect())
}

fn retained_history(
    context: &ExecutionWriterContext,
    attempt_id: &str,
) -> Result<work_operations::execution::file_transaction::history::Current, WorkError> {
    use work_operations::execution::file_transaction::history::{self, Entry};
    let store = LocalFileTransactions::new(context.writer().canonical_project_root.clone());
    let lineage = attempt_lineage(context, attempt_id)?;
    let mut frozen = Vec::new();
    for manifest in check_retained(
        &store.root,
        context.writer().requirement_id.as_str(),
        context.target().execution_dir,
    )? {
        let binding = LocalFileTransactions::binding(&manifest)?;
        if binding.task_id == context.target().task_id && lineage.contains(&binding.attempt_id) {
            let before = LocalFileTransactions::expected(&manifest, &binding, false)?;
            let after = LocalFileTransactions::expected(&manifest, &binding, true)?;
            let effective_identities = store.terminal_identities(&manifest)?;
            frozen.push((manifest, binding, before, after, effective_identities));
        }
    }
    let entries: Vec<_> = frozen
        .iter()
        .map(
            |(manifest, binding, before, after, effective_identities)| Entry {
                identity: &manifest.transaction_identity,
                previous: binding.previous_transaction.as_deref(),
                confirmation: binding.baseline_confirmation.as_ref(),
                path_identities: &binding.path_identities,
                effective_identities,
                phase: manifest.phase,
                before,
                after,
            },
        )
        .collect();
    history::current(&entries).map_err(error)
}

/// Resolve the exact version owned by a historical receipt without claiming it validates current files.
pub(in crate::execution) fn version_snapshot(
    context: &ExecutionWriterContext,
    attempt_id: &str,
    wanted: &str,
) -> Result<BTreeMap<String, Option<FileState>>, WorkError> {
    use work_operations::execution::file_transaction::history::{self, Entry};
    let current = retained_history(context, attempt_id)?;
    let store = LocalFileTransactions::new(context.writer().canonical_project_root.clone());
    let lineage = attempt_lineage(context, attempt_id)?;
    let mut transactions = BTreeMap::new();
    for manifest in check_retained(
        &store.root,
        context.writer().requirement_id.as_str(),
        context.target().execution_dir,
    )? {
        let binding = LocalFileTransactions::binding(&manifest)?;
        if binding.task_id == context.target().task_id && lineage.contains(&binding.attempt_id) {
            let before = LocalFileTransactions::expected(&manifest, &binding, false)?;
            let after = LocalFileTransactions::expected(&manifest, &binding, true)?;
            transactions.insert(manifest.transaction_identity.clone(), {
                let effective_identities = store.terminal_identities(&manifest)?;
                (manifest, binding, before, after, effective_identities)
            });
        }
    }
    let mut chain = Vec::new();
    let mut head = (!current.head.is_empty()).then_some(current.head.as_str());
    while let Some(id) = head {
        let row = transactions
            .get(id)
            .ok_or_else(|| error("file_transaction_history_disconnected"))?;
        chain.push(row);
        head = row.1.previous_transaction.as_deref();
    }
    chain.reverse();
    for count in 0..=chain.len() {
        let entries: Vec<_> = chain
            .iter()
            .take(count)
            .map(
                |(manifest, binding, before, after, effective_identities)| Entry {
                    identity: &manifest.transaction_identity,
                    previous: binding.previous_transaction.as_deref(),
                    confirmation: binding.baseline_confirmation.as_ref(),
                    path_identities: &binding.path_identities,
                    effective_identities,
                    phase: manifest.phase,
                    before,
                    after,
                },
            )
            .collect();
        let past = history::current(&entries).map_err(error)?;
        if history::version(&past) == wanted {
            let mut snapshot = current.initial;
            snapshot.extend(past.expected);
            return Ok(snapshot);
        }
    }
    Err(error("file_transaction_evidence_version_unknown"))
}

pub(in crate::execution) fn current_version(
    context: &ExecutionWriterContext,
    attempt_id: &str,
) -> Result<String, WorkError> {
    let store = LocalFileTransactions::new(context.writer().canonical_project_root.clone());
    let current = retained_history(context, attempt_id)?;
    for (path, expected) in &current.expected {
        if store.inspect(path)? != *expected
            || store.path_identity(path)? != current.path_identities[path]
        {
            return Err(error("file_transaction_target_drift"));
        }
    }
    Ok(work_operations::execution::file_transaction::history::version(&current))
}

/// Ground an AI diff reference in a complete retained transaction of this exact owner.
pub(in crate::execution) fn verify_change_reference(
    context: &ExecutionWriterContext,
    attempt_id: &str,
    transaction: &str,
    path: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<(), WorkError> {
    retained_history(context, attempt_id)?;
    let store = LocalFileTransactions::new(context.writer().canonical_project_root.clone());
    let manifest = store.load(context.writer().requirement_id.as_str(), transaction)?;
    let binding = LocalFileTransactions::binding(&manifest)?;
    if binding.task_id != context.target().task_id
        || binding.attempt_id != attempt_id
        || manifest.phase != RuntimePhase::PublishedVerified
    {
        return Err(error("validation_review_reference_unverified"));
    }
    let target = manifest
        .targets
        .iter()
        .find(|target| target.path == path)
        .ok_or_else(|| error("validation_review_reference_unverified"))?;
    if target.before.as_ref().map(|b| b.sha256.as_str()) != before
        || target.after.as_ref().map(|b| b.sha256.as_str()) != after
    {
        return Err(error("validation_review_reference_unverified"));
    }
    Ok(())
}

pub fn require_published(
    context: &ExecutionWriterContext,
    attempt_id: &str,
) -> Result<(), WorkError> {
    published_before(context, attempt_id).map(|_| ())
}

/// Original input bytes require complete retained publication from the verified
/// continuation lineage and current after bytes, metadata and path identities.
pub(crate) fn published_before(
    context: &ExecutionWriterContext,
    attempt_id: &str,
) -> Result<BTreeMap<String, Option<FileState>>, WorkError> {
    let current = retained_history(context, attempt_id)?;
    if current.published_paths.is_empty() {
        return Err(error("file_transaction_publication_required"));
    }
    let task = context.task_context().contract["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|task| task["id"] == context.target().task_id)
        .ok_or_else(|| error("file_transaction_task_missing"))?;
    for file in task["files"].as_array().into_iter().flatten() {
        for path in ["path", "source", "destination"]
            .iter()
            .filter_map(|key| file[*key].as_str())
        {
            if !current.published_paths.contains(path) {
                return Err(error("file_transaction_incomplete_set"));
            }
        }
    }
    current_version(context, attempt_id)?;
    Ok(current.initial)
}

/// Old approved record transactions retain forward recovery. A retained project-file
/// transaction for this Attempt must still prove its complete outcome during closure recovery.
pub fn require_retained_publication(
    context: &ExecutionWriterContext,
    attempt_id: &str,
) -> Result<(), WorkError> {
    let lineage = attempt_lineage(context, attempt_id)?;
    for manifest in check_retained(
        &context.writer().canonical_project_root,
        context.writer().requirement_id.as_str(),
        context.target().execution_dir,
    )? {
        let binding = LocalFileTransactions::binding(&manifest)?;
        if binding.task_id == context.target().task_id && lineage.contains(&binding.attempt_id) {
            return require_published(context, attempt_id);
        }
    }
    Ok(())
}
