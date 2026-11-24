//! Port to the formal record store. Implemented by `work-infrastructure`.

use work_model::{
    CorrectionRecord, DiscussionRecord, ExecutionRecord, RecordId, RecordKind,
    RequirementAcceptanceRecord, SourceRecord, TaskAcceptanceRecord, TaskRecord, WorkError,
};

/// One published version of a record, typed by kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoredRecord {
    Source(SourceRecord),
    Discussion(DiscussionRecord),
    Correction(CorrectionRecord),
    Task(TaskRecord),
    Execution(ExecutionRecord),
    TaskAcceptance(TaskAcceptanceRecord),
    RequirementAcceptance(RequirementAcceptanceRecord),
}

macro_rules! each_record {
    ($value:expr, $record:ident => $body:expr) => {
        match $value {
            StoredRecord::Source($record) => $body,
            StoredRecord::Discussion($record) => $body,
            StoredRecord::Correction($record) => $body,
            StoredRecord::Task($record) => $body,
            StoredRecord::Execution($record) => $body,
            StoredRecord::TaskAcceptance($record) => $body,
            StoredRecord::RequirementAcceptance($record) => $body,
        }
    };
}

impl StoredRecord {
    pub fn kind(&self) -> RecordKind {
        match self {
            Self::Source(_) => RecordKind::Source,
            Self::Discussion(_) => RecordKind::Discussion,
            Self::Correction(_) => RecordKind::Correction,
            Self::Task(_) => RecordKind::Task,
            Self::Execution(_) => RecordKind::Execution,
            Self::TaskAcceptance(_) => RecordKind::TaskAcceptance,
            Self::RequirementAcceptance(_) => RecordKind::RequirementAcceptance,
        }
    }

    pub fn id(&self) -> &RecordId {
        each_record!(self, record => &record.id)
    }

    pub fn version(&self) -> u32 {
        each_record!(self, record => record.version)
    }

    pub fn schema(&self) -> &str {
        each_record!(self, record => &record.schema)
    }
}

/// Latest version a record must still have when the plan is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Precondition {
    pub kind: RecordKind,
    pub id: RecordId,
    pub latest: Option<u32>,
}

pub trait RecordStore {
    /// Compute the digest of raw Source bytes in the technical adapter.
    fn content_digest(&self, bytes: &[u8]) -> String;

    /// Latest published version, or `None` when the record does not exist.
    fn latest_version(&self, kind: RecordKind, id: &RecordId) -> Result<Option<u32>, WorkError>;

    /// Read one published version; a missing version is `NotFound`.
    fn read(
        &self,
        kind: RecordKind,
        id: &RecordId,
        version: u32,
    ) -> Result<StoredRecord, WorkError>;

    /// Identifiers of every existing record of one kind, in stable order.
    fn ids(&self, kind: RecordKind) -> Result<Vec<RecordId>, WorkError>;

    /// Publish one new version atomically, checking all preconditions under the write lock.
    /// A failure after publication does not undo the published version.
    fn write(&self, record: &StoredRecord, preconditions: &[Precondition])
    -> Result<(), WorkError>;
}
