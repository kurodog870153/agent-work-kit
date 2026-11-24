use std::fs;
use std::path::PathBuf;

use work_feature::{Precondition, RecordStore, StoredRecord};
use work_model::{
    DISCUSSION_SCHEMA, DiscussionRecord, DiscussionStatus, Origin, OriginSource, Reason, RecordId,
    RecordKind, SOURCE_SCHEMA, SourceContent, SourceRecord, WorkError,
};

use super::{Fault, FsStore, WRITE_TMP};
use crate::digest::digest;

struct Dir(PathBuf);

impl Dir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "work-store-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        ));
        Self(path)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn id(value: &str) -> RecordId {
    RecordId::parse(value).unwrap()
}

fn source(name: &str, content: &str) -> StoredRecord {
    StoredRecord::Source(SourceRecord {
        schema: SOURCE_SCHEMA.to_owned(),
        id: id(name),
        version: 1,
        title: "t".to_owned(),
        locator: "l".to_owned(),
        content: SourceContent::Text {
            text: content.to_owned(),
        },
        content_length: content.len() as u64,
        content_digest: digest(content.as_bytes()),
        origin: Origin {
            source: OriginSource::External,
            reference: None,
        },
    })
}

fn discussion(name: &str, version: u32) -> StoredRecord {
    StoredRecord::Discussion(DiscussionRecord {
        schema: DISCUSSION_SCHEMA.to_owned(),
        id: id(name),
        version,
        original_request: "r".to_owned(),
        status: DiscussionStatus::Open,
        source_ids: Vec::new(),
        questions: Vec::new(),
        answers: Vec::new(),
        decisions: Vec::new(),
        pending: Vec::new(),
    })
}

fn create(store: &FsStore, record: StoredRecord) -> Result<(), WorkError> {
    store.write(
        &record,
        &[Precondition {
            kind: record.kind(),
            id: record.id().clone(),
            latest: None,
        }],
    )
}

fn append(store: &FsStore, record: StoredRecord, latest: u32) -> Result<(), WorkError> {
    store.write(
        &record,
        &[Precondition {
            kind: record.kind(),
            id: record.id().clone(),
            latest: Some(latest),
        }],
    )
}

#[test]
fn write_and_read_round_trip_keeps_original_bytes() {
    let dir = Dir::new("round");
    let store = FsStore::open(&dir.0);
    assert_eq!(
        store.latest_version(RecordKind::Source, &id("a")).unwrap(),
        None
    );
    create(&store, source("a", "  x\r\n\ty  ")).unwrap();
    assert_eq!(
        store.latest_version(RecordKind::Source, &id("a")).unwrap(),
        Some(1)
    );
    assert_eq!(
        store.read(RecordKind::Source, &id("a"), 1).unwrap(),
        source("a", "  x\r\n\ty  ")
    );
    assert_eq!(store.ids(RecordKind::Source).unwrap(), vec![id("a")]);
    assert!(!dir.0.join(WRITE_TMP).exists());
}

#[test]
fn published_versions_are_never_overwritten() {
    let dir = Dir::new("immutable");
    let store = FsStore::open(&dir.0);
    create(&store, source("a", "original")).unwrap();
    let again = create(&store, source("a", "replacement")).unwrap_err();
    assert_eq!(again.reason, Reason::AlreadyExists);
    assert_eq!(
        store
            .write(&source("a", "replacement"), &[])
            .unwrap_err()
            .reason,
        Reason::AlreadyExists
    );
    assert_eq!(
        store.read(RecordKind::Source, &id("a"), 1).unwrap(),
        source("a", "original")
    );
}

#[test]
fn stale_preconditions_reject_the_write() {
    let dir = Dir::new("stale");
    let store = FsStore::open(&dir.0);
    create(&store, discussion("d", 1)).unwrap();
    append(&store, discussion("d", 2), 1).unwrap();
    let stale = append(&store, discussion("d", 2), 1).unwrap_err();
    assert_eq!(stale.reason, Reason::VersionConflict);
    let conditions = [
        Precondition {
            kind: RecordKind::Discussion,
            id: id("d"),
            latest: Some(2),
        },
        Precondition {
            kind: RecordKind::Source,
            id: id("never"),
            latest: Some(1),
        },
    ];
    assert_eq!(
        store
            .write(&discussion("d", 3), &conditions)
            .unwrap_err()
            .reason,
        Reason::NotFound
    );
    assert_eq!(
        store
            .latest_version(RecordKind::Discussion, &id("d"))
            .unwrap(),
        Some(2)
    );
    assert_eq!(
        store
            .latest_version(RecordKind::Source, &id("ghost-owner"))
            .unwrap(),
        None
    );
}

#[test]
fn interrupted_before_publish_leaves_no_version_and_next_write_cleans_temporary() {
    let dir = Dir::new("before-publish");
    let store = FsStore::open(&dir.0);
    create(&store, source("keep", "kept")).unwrap();
    store.fault.set(Some(Fault::InterruptBeforePublish));
    assert!(create(&store, source("lost", "x")).is_err());
    store.fault.set(None);
    assert!(dir.0.join(WRITE_TMP).exists());
    assert_eq!(store.ids(RecordKind::Source).unwrap(), vec![id("keep")]);
    assert_eq!(
        store
            .latest_version(RecordKind::Source, &id("lost"))
            .unwrap(),
        None
    );
    create(&store, source("lost", "new")).unwrap();
    assert!(!dir.0.join(WRITE_TMP).exists());
    assert!(!dir.0.join("txn").exists());
    assert_eq!(
        store.read(RecordKind::Source, &id("lost"), 1).unwrap(),
        source("lost", "new")
    );
}

#[test]
fn interrupted_after_publish_preserves_version_and_next_write_only_unlinks_temporary() {
    let dir = Dir::new("after-publish");
    let store = FsStore::open(&dir.0);
    store.fault.set(Some(Fault::InterruptAfterPublish));
    assert!(create(&store, source("a", "original")).is_err());
    store.fault.set(None);
    assert!(dir.0.join(WRITE_TMP).exists());
    assert_eq!(
        store.read(RecordKind::Source, &id("a"), 1).unwrap(),
        source("a", "original")
    );
    create(&store, source("b", "next")).unwrap();
    assert!(!dir.0.join(WRITE_TMP).exists());
    assert_eq!(
        store.read(RecordKind::Source, &id("a"), 1).unwrap(),
        source("a", "original")
    );
    assert_eq!(
        create(&store, source("a", "replacement"))
            .unwrap_err()
            .reason,
        Reason::AlreadyExists
    );
}

#[test]
fn failed_readback_preserves_published_record() {
    let dir = Dir::new("readback");
    let store = FsStore::open(&dir.0);
    store.fault.set(Some(Fault::CorruptReadback));
    assert_eq!(
        create(&store, source("a", "x")).unwrap_err().reason,
        Reason::StorageFailure
    );
    store.fault.set(None);
    assert!(!dir.0.join(WRITE_TMP).exists());
    assert_eq!(
        store.read(RecordKind::Source, &id("a"), 1).unwrap(),
        source("a", "x")
    );
}

#[test]
fn a_second_writer_is_refused_while_the_root_is_locked() {
    let dir = Dir::new("lock");
    let store = FsStore::open(&dir.0);
    create(&store, source("a", "x")).unwrap();
    let holder = fs::OpenOptions::new()
        .write(true)
        .open(dir.0.join("lock"))
        .unwrap();
    holder.lock().unwrap();
    let mut contender = FsStore::open(&dir.0);
    contender.lock_attempts = 2;
    assert_eq!(
        create(&contender, source("b", "y")).unwrap_err().reason,
        Reason::Locked
    );
    assert_eq!(
        contender.ids(RecordKind::Source).unwrap_err().reason,
        Reason::Locked
    );
    drop(holder);
    create(&contender, source("b", "y")).unwrap();
}

#[test]
fn edited_content_of_valid_json_is_rejected_in_latest_and_history() {
    let dir = Dir::new("digest");
    let store = FsStore::open(&dir.0);
    create(&store, discussion("d", 1)).unwrap();
    append(&store, discussion("d", 2), 1).unwrap();
    let records = dir.0.join("records").join("discussion").join("d");
    for version in [1u32, 2] {
        assert!(
            store
                .read(RecordKind::Discussion, &id("d"), version)
                .is_ok()
        );
    }
    for version in [2u32, 1] {
        let path = records.join(format!("v{version:04}.json"));
        let text = fs::read_to_string(&path).unwrap();
        let edited = text.replace("\"original_request\": \"r\"", "\"original_request\": \"q\"");
        assert_ne!(text, edited);
        fs::write(&path, edited).unwrap();
        assert_eq!(
            store
                .read(RecordKind::Discussion, &id("d"), version)
                .unwrap_err()
                .reason,
            Reason::InvalidRecord
        );
    }
}

#[test]
fn damaged_stores_are_reported_not_trusted() {
    let dir = Dir::new("damage");
    let store = FsStore::open(&dir.0);
    create(&store, source("a", "x")).unwrap();
    create(&store, source("b", "y")).unwrap();
    let records = dir.0.join("records").join("source");

    fs::copy(
        records.join("b").join("v0001.json"),
        records.join("a").join("v0001.json"),
    )
    .unwrap();
    assert_eq!(
        store
            .read(RecordKind::Source, &id("a"), 1)
            .unwrap_err()
            .reason,
        Reason::InvalidRecord
    );

    fs::write(records.join("a").join("v0003.json"), "{}").unwrap();
    assert_eq!(
        store
            .latest_version(RecordKind::Source, &id("a"))
            .unwrap_err()
            .reason,
        Reason::InvalidRecord
    );
    assert_eq!(
        store
            .read(RecordKind::Source, &id("a"), 2)
            .unwrap_err()
            .reason,
        Reason::NotFound
    );
}

#[test]
fn binary_source_preserves_invalid_utf8_and_rejects_bad_length_or_digest() {
    let dir = Dir::new("binary");
    let store = FsStore::open(&dir.0);
    let bytes = vec![0, 255, 128, 13, 10, 0, 240];
    let binary = StoredRecord::Source(SourceRecord {
        schema: SOURCE_SCHEMA.to_owned(),
        id: id("raw"),
        version: 1,
        title: "attachment".to_owned(),
        locator: "uploaded".to_owned(),
        content: SourceContent::Binary {
            bytes: bytes.clone(),
        },
        content_length: bytes.len() as u64,
        content_digest: digest(&bytes),
        origin: Origin {
            source: OriginSource::External,
            reference: None,
        },
    });
    create(&store, binary.clone()).unwrap();
    assert_eq!(
        store.read(RecordKind::Source, &id("raw"), 1).unwrap(),
        binary
    );
    let file = dir.0.join("records/source/raw/v0001.json");
    let StoredRecord::Source(original) = binary else {
        unreachable!()
    };
    let mut invalid_length = original.clone();
    invalid_length.content_length += 1;
    fs::write(&file, super::seal(&invalid_length).unwrap()).unwrap();
    assert_eq!(
        store
            .read(RecordKind::Source, &id("raw"), 1)
            .unwrap_err()
            .reason,
        Reason::InvalidRecord
    );
    let mut invalid_digest = original.clone();
    invalid_digest.content_digest = "not-the-digest".to_owned();
    fs::write(&file, super::seal(&invalid_digest).unwrap()).unwrap();
    assert_eq!(
        store
            .read(RecordKind::Source, &id("raw"), 1)
            .unwrap_err()
            .reason,
        Reason::InvalidRecord
    );
    // Invalid source content must not enter a published version through a write.
    let mut broken = invalid_digest;
    broken.id = id("broken");
    assert_eq!(
        create(&store, StoredRecord::Source(broken))
            .unwrap_err()
            .reason,
        Reason::InvalidRecord
    );
    assert_eq!(
        store
            .latest_version(RecordKind::Source, &id("broken"))
            .unwrap(),
        None
    );
}

#[test]
fn binary_source_failed_readback_preserves_published_record() {
    let dir = Dir::new("binary-readback");
    let store = FsStore::open(&dir.0);
    let bytes = vec![0, 255, 128, 0];
    let source = StoredRecord::Source(SourceRecord {
        schema: SOURCE_SCHEMA.to_owned(),
        id: id("raw"),
        version: 1,
        title: "attachment".to_owned(),
        locator: "uploaded".to_owned(),
        content: SourceContent::Binary {
            bytes: bytes.clone(),
        },
        content_length: bytes.len() as u64,
        content_digest: digest(&bytes),
        origin: Origin {
            source: OriginSource::External,
            reference: None,
        },
    });
    store.fault.set(Some(Fault::CorruptReadback));
    assert_eq!(
        create(&store, source.clone()).unwrap_err().reason,
        Reason::StorageFailure
    );
    store.fault.set(None);
    assert_eq!(
        store
            .latest_version(RecordKind::Source, &id("raw"))
            .unwrap(),
        Some(1)
    );
    assert_eq!(
        store.read(RecordKind::Source, &id("raw"), 1).unwrap(),
        source
    );
}

#[test]
fn reviewed_dependency_is_re_read_under_the_write_lock() {
    let dir = Dir::new("reviewed-dependency");
    let store = FsStore::open(&dir.0);
    create(&store, source("s1", "original")).unwrap();
    let path = dir.0.join("records/source/s1/v0001.json");
    fs::write(&path, "invalid published content").unwrap();

    let conditions = [
        Precondition {
            kind: RecordKind::Discussion,
            id: id("d1"),
            latest: None,
        },
        Precondition {
            kind: RecordKind::Source,
            id: id("s1"),
            latest: Some(1),
        },
    ];
    assert_eq!(
        store
            .write(&discussion("d1", 1), &conditions)
            .unwrap_err()
            .reason,
        Reason::InvalidRecord
    );
    assert_eq!(
        store
            .latest_version(RecordKind::Discussion, &id("d1"))
            .unwrap(),
        None
    );
}
