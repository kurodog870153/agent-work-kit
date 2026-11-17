//! File-system adapter of the formal record store.
//!
//! Layout under the Work root:
//! `records/<kind>/<id>/v0001.json` for every published version, `lock` for
//! mutual exclusion and `txn/` for the journal and staged files of the
//! transaction in flight. Published version files are only ever created, never
//! rewritten.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use work_feature::{RecordStore, StoredRecord, WritePlan};
use work_model::{
    CORRECTION_SCHEMA, DISCUSSION_SCHEMA, EXECUTION_SCHEMA, REQUIREMENT_ACCEPTANCE_SCHEMA, Reason,
    RecordEnvelope, RecordId, RecordKind, SOURCE_SCHEMA, SourceRecord, TASK_ACCEPTANCE_SCHEMA,
    TASK_SCHEMA, WorkError,
};
use work_operations::{decode, encode};

use crate::digest::digest;

const LOCK_FILE: &str = "lock";
const RECORDS: &str = "records";
const TXN: &str = "txn";
const JOURNAL: &str = "journal";
const JOURNAL_TMP: &str = "journal.tmp";
const STAGE: &str = "stage";
const LOCK_ATTEMPTS: u32 = 100;
const LOCK_WAIT: Duration = Duration::from_millis(20);

/// Failure points a test can force; production code never sets one.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fault {
    /// Stop right after the first target was placed, as a crash would.
    InterruptAfterFirstPlace,
    /// Make the readback of the first target disagree with the plan.
    CorruptReadback,
}

#[derive(Debug)]
pub struct FsStore {
    root: PathBuf,
    lock_attempts: u32,
    #[cfg(test)]
    fault: std::cell::Cell<Option<Fault>>,
}

fn storage(error: &std::io::Error, context: &str) -> WorkError {
    WorkError::new(Reason::StorageFailure, format!("{context}: {error}"))
}

/// Encode a record into its envelope text with the digest of its canonical text.
fn seal<T: Serialize>(record: &T) -> Result<String, WorkError> {
    let digest = digest(encode(record)?.as_bytes());
    encode(&RecordEnvelope { digest, record })
}

/// Decode an envelope and recompute the digest; a mismatch is an invalid record.
fn unseal<T: Serialize + DeserializeOwned>(text: &str) -> Result<T, WorkError> {
    let envelope: RecordEnvelope<T> = decode(text)?;
    if digest(encode(&envelope.record)?.as_bytes()) != envelope.digest {
        return Err(WorkError::new(
            Reason::InvalidRecord,
            "record content does not match its stored digest",
        ));
    }
    Ok(envelope.record)
}

/// Check the independent digest and byte count of original Source evidence.
fn validate_source(record: &SourceRecord) -> Result<(), WorkError> {
    let bytes = record.content.bytes();
    if record.content_length != bytes.len() as u64 || record.content_digest != digest(bytes) {
        return Err(WorkError::new(
            Reason::InvalidRecord,
            "Source content length or digest does not match its original bytes",
        ));
    }
    Ok(())
}

fn version_name(version: u32) -> String {
    format!("v{version:04}.json")
}

fn parse_version(name: &str) -> Option<u32> {
    name.strip_prefix('v')?.strip_suffix(".json")?.parse().ok()
}

impl FsStore {
    /// Open a store at `root`; the directory is created on the first write.
    pub fn open(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            lock_attempts: LOCK_ATTEMPTS,
            #[cfg(test)]
            fault: std::cell::Cell::new(None),
        }
    }

    fn records_dir(&self, kind: RecordKind, id: &RecordId) -> PathBuf {
        self.root.join(RECORDS).join(kind.code()).join(id.as_str())
    }

    fn txn_dir(&self) -> PathBuf {
        self.root.join(TXN)
    }

    fn lock(&self, exclusive: bool) -> Result<Option<File>, WorkError> {
        if exclusive {
            fs::create_dir_all(&self.root).map_err(|e| storage(&e, "create root"))?;
        } else if !self.root.is_dir() {
            return Ok(None);
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(self.root.join(LOCK_FILE))
            .map_err(|e| storage(&e, "open lock"))?;
        for attempt in 0..self.lock_attempts {
            let outcome = if exclusive {
                file.try_lock()
            } else {
                file.try_lock_shared()
            };
            match outcome {
                Ok(()) => return Ok(Some(file)),
                Err(TryLockError::WouldBlock) => {
                    if attempt + 1 < self.lock_attempts {
                        sleep(LOCK_WAIT);
                    }
                }
                Err(TryLockError::Error(error)) => return Err(storage(&error, "lock")),
            }
        }
        Err(WorkError::new(
            Reason::Locked,
            "another process holds the Work root",
        ))
    }

    fn ensure_clean(&self) -> Result<(), WorkError> {
        let txn = self.txn_dir();
        if txn.join(JOURNAL).exists() || txn.join(JOURNAL_TMP).exists() || txn.join(STAGE).exists()
        {
            return Err(WorkError::new(
                Reason::RecoveryRequired,
                "an interrupted transaction is pending; run recover",
            ));
        }
        Ok(())
    }

    fn versions_of(&self, kind: RecordKind, id: &RecordId) -> Result<Vec<u32>, WorkError> {
        let directory = self.records_dir(kind, id);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(storage(&error, "list versions")),
        };
        let mut versions = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| storage(&e, "list versions"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            versions.push(parse_version(&name).ok_or_else(|| {
                WorkError::new(
                    Reason::InvalidRecord,
                    format!("unexpected entry in {}: {name}", directory.display()),
                )
            })?);
        }
        versions.sort_unstable();
        if versions
            .iter()
            .zip(1u32..)
            .any(|(version, expected)| *version != expected)
        {
            return Err(WorkError::new(
                Reason::InvalidRecord,
                format!(
                    "version history of {} {} is not contiguous",
                    kind.code(),
                    id.as_str()
                ),
            ));
        }
        Ok(versions)
    }

    fn read_unlocked(
        &self,
        kind: RecordKind,
        id: &RecordId,
        version: u32,
    ) -> Result<StoredRecord, WorkError> {
        let path = self.records_dir(kind, id).join(version_name(version));
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(WorkError::new(
                    Reason::NotFound,
                    format!("{} {} has no version {version}", kind.code(), id.as_str()),
                ));
            }
            Err(error) => return Err(storage(&error, "read record")),
        };
        let record = match kind {
            RecordKind::Source => {
                let record: SourceRecord = unseal(&text)?;
                validate_source(&record)?;
                StoredRecord::Source(record)
            }
            RecordKind::Discussion => StoredRecord::Discussion(unseal(&text)?),
            RecordKind::Correction => StoredRecord::Correction(unseal(&text)?),
            RecordKind::Task => StoredRecord::Task(unseal(&text)?),
            RecordKind::Execution => StoredRecord::Execution(unseal(&text)?),
            RecordKind::TaskAcceptance => StoredRecord::TaskAcceptance(unseal(&text)?),
            RecordKind::RequirementAcceptance => {
                StoredRecord::RequirementAcceptance(unseal(&text)?)
            }
        };
        let expected_schema = match kind {
            RecordKind::Source => SOURCE_SCHEMA,
            RecordKind::Discussion => DISCUSSION_SCHEMA,
            RecordKind::Correction => CORRECTION_SCHEMA,
            RecordKind::Task => TASK_SCHEMA,
            RecordKind::Execution => EXECUTION_SCHEMA,
            RecordKind::TaskAcceptance => TASK_ACCEPTANCE_SCHEMA,
            RecordKind::RequirementAcceptance => REQUIREMENT_ACCEPTANCE_SCHEMA,
        };
        if record.schema() != expected_schema || record.id() != id || record.version() != version {
            return Err(WorkError::new(
                Reason::InvalidRecord,
                format!(
                    "{} {} v{version} does not match its stored identity",
                    kind.code(),
                    id.as_str()
                ),
            ));
        }
        Ok(record)
    }

    fn rollback(&self) -> Result<Vec<String>, WorkError> {
        let txn = self.txn_dir();
        let mut removed = Vec::new();
        if let Ok(journal) = fs::read_to_string(txn.join(JOURNAL)) {
            for relative in journal.lines().filter(|line| !line.is_empty()) {
                let target = self.root.join(relative);
                match fs::remove_file(&target) {
                    Ok(()) => removed.push(relative.to_owned()),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(storage(&error, "roll back target")),
                }
                if let Some(parent) = target.parent() {
                    let _ = fs::remove_dir(parent);
                }
            }
        }
        for name in [JOURNAL, JOURNAL_TMP] {
            match fs::remove_file(txn.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(storage(&error, "remove journal")),
            }
        }
        match fs::remove_dir_all(txn.join(STAGE)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(storage(&error, "remove stage")),
        }
        Ok(removed)
    }

    fn execute(&self, planned: &[(PathBuf, String)]) -> Result<(), WorkError> {
        let txn = self.txn_dir();
        let stage = txn.join(STAGE);
        fs::create_dir_all(&stage).map_err(|e| storage(&e, "create stage"))?;
        for (index, (_, text)) in planned.iter().enumerate() {
            let mut file =
                File::create(stage.join(index.to_string())).map_err(|e| storage(&e, "stage"))?;
            file.write_all(text.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|e| storage(&e, "stage"))?;
        }
        let journal: String = planned
            .iter()
            .map(|(relative, _)| format!("{}\n", relative.to_string_lossy().replace('\\', "/")))
            .collect();
        fs::write(txn.join(JOURNAL_TMP), journal).map_err(|e| storage(&e, "write journal"))?;
        fs::rename(txn.join(JOURNAL_TMP), txn.join(JOURNAL))
            .map_err(|e| storage(&e, "publish journal"))?;

        for (index, (relative, _)) in planned.iter().enumerate() {
            let target = self.root.join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| storage(&e, "create record dir"))?;
            }
            fs::rename(stage.join(index.to_string()), &target)
                .map_err(|e| storage(&e, "place record"))?;
            #[cfg(test)]
            if index == 0 && self.fault.get() == Some(Fault::InterruptAfterFirstPlace) {
                return Err(WorkError::new(Reason::StorageFailure, "interrupted"));
            }
        }
        for (index, (relative, text)) in planned.iter().enumerate() {
            let stored = fs::read_to_string(self.root.join(relative))
                .map_err(|e| storage(&e, "read back"))?;
            #[cfg(test)]
            let stored = if index == 0 && self.fault.get() == Some(Fault::CorruptReadback) {
                format!("{stored}!")
            } else {
                stored
            };
            #[cfg(not(test))]
            let _ = index;
            if &stored != text {
                return Err(WorkError::new(
                    Reason::StorageFailure,
                    format!("readback mismatch: {}", relative.display()),
                ));
            }
        }
        Ok(())
    }
}

impl RecordStore for FsStore {
    fn content_digest(&self, bytes: &[u8]) -> String {
        digest(bytes)
    }

    fn latest_version(&self, kind: RecordKind, id: &RecordId) -> Result<Option<u32>, WorkError> {
        let Some(_guard) = self.lock(false)? else {
            return Ok(None);
        };
        self.ensure_clean()?;
        Ok(self.versions_of(kind, id)?.last().copied())
    }

    fn read(
        &self,
        kind: RecordKind,
        id: &RecordId,
        version: u32,
    ) -> Result<StoredRecord, WorkError> {
        let Some(_guard) = self.lock(false)? else {
            return Err(WorkError::new(
                Reason::NotFound,
                format!("{} not found: {}", kind.code(), id.as_str()),
            ));
        };
        self.ensure_clean()?;
        self.read_unlocked(kind, id, version)
    }

    fn ids(&self, kind: RecordKind) -> Result<Vec<RecordId>, WorkError> {
        let Some(_guard) = self.lock(false)? else {
            return Ok(Vec::new());
        };
        self.ensure_clean()?;
        let entries = match fs::read_dir(self.root.join(RECORDS).join(kind.code())) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(storage(&error, "list records")),
        };
        let mut ids = Vec::new();
        for entry in entries {
            let name = entry
                .map_err(|e| storage(&e, "list records"))?
                .file_name()
                .to_string_lossy()
                .into_owned();
            ids.push(RecordId::parse(&name).map_err(|_| {
                WorkError::new(Reason::InvalidRecord, format!("unexpected entry: {name}"))
            })?);
        }
        ids.sort();
        Ok(ids)
    }

    fn commit(&self, plan: &WritePlan) -> Result<(), WorkError> {
        let _guard = self.lock(true)?;
        self.ensure_clean()?;
        for condition in &plan.preconditions {
            let actual = self
                .versions_of(condition.kind, &condition.id)?
                .last()
                .copied();
            if actual != condition.latest {
                let reason = if condition.latest.is_none() {
                    Reason::AlreadyExists
                } else if actual.is_none() {
                    Reason::NotFound
                } else {
                    Reason::VersionConflict
                };
                return Err(WorkError::new(
                    reason,
                    format!(
                        "{} {} is at {actual:?}, plan expected {:?}",
                        condition.kind.code(),
                        condition.id.as_str(),
                        condition.latest
                    ),
                ));
            }
        }
        // Under the same exclusive lock that commits and reads back the writes,
        // re-read all reviewed dependency versions and check their integrity.
        // A version number alone is not proof that its published content is valid.
        for condition in &plan.preconditions {
            if let Some(version) = condition.latest {
                self.read_unlocked(condition.kind, &condition.id, version)?;
            }
        }
        let mut planned: Vec<(PathBuf, String)> = Vec::new();
        for record in &plan.writes {
            let latest = self
                .versions_of(record.kind(), record.id())?
                .last()
                .copied()
                .unwrap_or(0);
            let relative = PathBuf::from(RECORDS)
                .join(record.kind().code())
                .join(record.id().as_str())
                .join(version_name(record.version()));
            if record.version() != latest + 1
                || planned.iter().any(|(path, _)| path == &relative)
                || self.root.join(&relative).exists()
            {
                return Err(WorkError::new(
                    Reason::AlreadyExists,
                    format!("published history is immutable: {}", relative.display()),
                ));
            }
            let text = match record {
                StoredRecord::Source(value) => {
                    validate_source(value)?;
                    seal(value)?
                }
                StoredRecord::Discussion(value) => seal(value)?,
                StoredRecord::Correction(value) => seal(value)?,
                StoredRecord::Task(value) => seal(value)?,
                StoredRecord::Execution(value) => seal(value)?,
                StoredRecord::TaskAcceptance(value) => seal(value)?,
                StoredRecord::RequirementAcceptance(value) => seal(value)?,
            };
            planned.push((relative, text));
        }
        if let Err(error) = self.execute(&planned) {
            #[cfg(test)]
            if self.fault.get() == Some(Fault::InterruptAfterFirstPlace) {
                return Err(error);
            }
            return match self.rollback() {
                Ok(_) => Err(error),
                Err(rollback) => Err(WorkError::new(
                    Reason::RecoveryRequired,
                    format!("{error}; rollback failed: {rollback}"),
                )),
            };
        }
        self.rollback_markers()
    }

    fn recover(&self) -> Result<Vec<String>, WorkError> {
        let Some(_guard) = self.lock_for_recovery()? else {
            return Ok(Vec::new());
        };
        self.rollback()
    }
}

impl FsStore {
    /// Remove the journal and stage of a transaction that completed.
    fn rollback_markers(&self) -> Result<(), WorkError> {
        let txn = self.txn_dir();
        fs::remove_file(txn.join(JOURNAL)).map_err(|e| storage(&e, "remove journal"))?;
        fs::remove_dir_all(txn.join(STAGE)).map_err(|e| storage(&e, "remove stage"))
    }

    fn lock_for_recovery(&self) -> Result<Option<File>, WorkError> {
        if !self.root.is_dir() {
            return Ok(None);
        }
        self.lock(true)
    }
}

#[cfg(test)]
mod tests;
