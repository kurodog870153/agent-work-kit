//! File-system adapter of the formal record store.
//!
//! Published versions live at records/<kind>/<id>/v0001.json. A root lock
//! protects preconditions, publication and readback; write.tmp is unpublished.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;
use work_feature::{Precondition, RecordStore, StoredRecord};
use work_model::{
    CORRECTION_SCHEMA, DISCUSSION_SCHEMA, EXECUTION_SCHEMA, REQUIREMENT_ACCEPTANCE_SCHEMA, Reason,
    RecordEnvelope, RecordId, RecordKind, SOURCE_SCHEMA, SourceRecord, TASK_ACCEPTANCE_SCHEMA,
    TASK_SCHEMA, WorkError,
};
use work_operations::{decode, encode};

use crate::digest::digest;

const LOCK_FILE: &str = "lock";
const RECORDS: &str = "records";
const WRITE_TMP: &str = "write.tmp";
const LOCK_ATTEMPTS: u32 = 100;
const LOCK_WAIT: Duration = Duration::from_millis(20);

/// Failure points a test can force; production code never sets one.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Fault {
    /// Stop after syncing the temporary file, before publication.
    InterruptBeforePublish,
    /// Stop after atomic publication, before temporary-file cleanup.
    InterruptAfterPublish,
    /// Make the readback disagree with the published content.
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

    fn clear_temporary(&self) -> Result<(), WorkError> {
        match fs::remove_file(self.root.join(WRITE_TMP)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(storage(&error, "remove temporary write")),
        }
    }

    // Caller holds the root exclusive lock. A hard link publishes a complete
    // file in one operation without replacing an existing destination on
    // either Windows or macOS. Unsupported/cross-volume filesystems fail closed.
    fn publish(&self, target: &Path, text: &str) -> Result<(), WorkError> {
        let temporary = self.root.join(WRITE_TMP);
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|e| storage(&e, "create temporary write"))?;
            file.write_all(text.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(|e| storage(&e, "sync temporary write"))?;
            drop(file);
            let staged = fs::read_to_string(&temporary)
                .map_err(|e| storage(&e, "verify temporary write"))?;
            if staged != text {
                return Err(WorkError::new(
                    Reason::StorageFailure,
                    "temporary write mismatch",
                ));
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| storage(&e, "create record dir"))?;
            }
            #[cfg(test)]
            if self.fault.get() == Some(Fault::InterruptBeforePublish) {
                return Err(WorkError::new(
                    Reason::StorageFailure,
                    "interrupted before publication",
                ));
            }
            fs::hard_link(&temporary, target).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    WorkError::new(Reason::AlreadyExists, "published history is immutable")
                } else {
                    storage(&error, "publish record")
                }
            })?;
            #[cfg(test)]
            if self.fault.get() == Some(Fault::InterruptAfterPublish) {
                return Err(WorkError::new(
                    Reason::StorageFailure,
                    "interrupted after publication",
                ));
            }
            let stored = fs::read_to_string(target).map_err(|e| storage(&e, "read back"))?;
            #[cfg(test)]
            let stored = if self.fault.get() == Some(Fault::CorruptReadback) {
                format!("{stored}!")
            } else {
                stored
            };
            if stored != text {
                return Err(WorkError::new(Reason::StorageFailure, "readback mismatch"));
            }
            Ok(())
        })();
        #[cfg(test)]
        if matches!(
            self.fault.get(),
            Some(Fault::InterruptBeforePublish | Fault::InterruptAfterPublish)
        ) {
            return result;
        }
        let cleanup = self.clear_temporary();
        result.and(cleanup)
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
        self.read_unlocked(kind, id, version)
    }

    fn ids(&self, kind: RecordKind) -> Result<Vec<RecordId>, WorkError> {
        let Some(_guard) = self.lock(false)? else {
            return Ok(Vec::new());
        };
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
            let id = RecordId::parse(&name).map_err(|_| {
                WorkError::new(Reason::InvalidRecord, format!("unexpected entry: {name}"))
            })?;
            if !self.versions_of(kind, &id)?.is_empty() {
                ids.push(id);
            }
        }
        ids.sort();
        Ok(ids)
    }

    fn write(
        &self,
        record: &StoredRecord,
        preconditions: &[Precondition],
    ) -> Result<(), WorkError> {
        let _guard = self.lock(true)?;
        for condition in preconditions {
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
        // Under the same exclusive lock that publishes and reads back the record,
        // re-read all reviewed dependency versions and check their integrity.
        // A version number alone is not proof that its published content is valid.
        for condition in preconditions {
            if let Some(version) = condition.latest {
                self.read_unlocked(condition.kind, &condition.id, version)?;
            }
        }
        let latest = self
            .versions_of(record.kind(), record.id())?
            .last()
            .copied()
            .unwrap_or(0);
        let target = self
            .records_dir(record.kind(), record.id())
            .join(version_name(record.version()));
        if latest.checked_add(1) != Some(record.version()) || target.exists() {
            return Err(WorkError::new(
                Reason::AlreadyExists,
                format!("published history is immutable: {}", target.display()),
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
        self.clear_temporary()?;
        self.publish(&target, &text)
    }
}

#[cfg(test)]
mod tests;
