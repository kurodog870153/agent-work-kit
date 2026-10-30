//! Artifact fingerprint entry points. Byte primitives stay in `canonical`.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use work_model::task::index::TaskItemReference;

use crate::canonical::{
    InstructionSource, canonical_json_sha256, canonical_sha256, instructions_sha256, sha256_hex,
};
use crate::skill::BundleEntry;
use crate::task::collection::collection_fingerprint_sha256;

pub fn raw(raw: &[u8]) -> String {
    sha256_hex(raw)
}

/// Hash descriptor bytes with constant working memory, preserving the raw SHA-256 contract.
pub fn raw_reader(mut reader: impl std::io::Read) -> std::io::Result<(u64, String)> {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    let mut length = 0_u64;
    let mut buffer = [0_u8; 65536];
    loop {
        let count = match reader.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            break;
        }
        length = length
            .checked_add(count as u64)
            .ok_or_else(|| std::io::Error::other("input length overflow"))?;
        hash.update(&buffer[..count]);
    }
    let digest = hash.finalize();
    let mut hexadecimal = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write;
        write!(&mut hexadecimal, "{byte:02x}").expect("writing to String cannot fail");
    }
    Ok((length, hexadecimal))
}

pub fn verify_raw(raw: &[u8], expected: &str) -> bool {
    self::raw(raw) == expected
}

pub fn ledger(raw: &[u8]) -> String {
    sha256_hex(raw)
}

pub fn journal(raw: &[u8]) -> String {
    sha256_hex(raw)
}

pub fn history(bytes: &[u8]) -> String {
    raw(bytes)
}

pub fn skill_bundle(entries: &[BundleEntry<'_>]) -> String {
    skill_api::bundle::bundle("WORK-SKILL-BUNDLE-SHA-256-V1", entries)
}

pub fn skill_identity(name: &str, scope: &str, root: &str, source: &str) -> String {
    skill_api::bundle::identity("WORK-SKILL-IDENTITY-V1", name, scope, root, source)
}

pub fn skill_selection(decision: &str, skills: &[Value]) -> String {
    skill_api::bundle::selection(decision, skills)
}

/// Fingerprints of the complete installed Specification baseline.
pub fn specification_baseline(
    source_sha256: &str,
    index_raw: &[u8],
    execution_raw: &[u8],
    items: &BTreeMap<String, Vec<u8>>,
) -> Value {
    json!({"source_sha256": source_sha256,
        "task_index_sha256": raw(index_raw),
        "execution_index_sha256": raw(execution_raw),
        "task_item_sha256": items.iter().map(|(id, bytes)|
            (id.clone(), raw(bytes))).collect::<BTreeMap<_, _>>()})
}

pub fn verify_ledger(raw: &[u8], expected: &str) -> bool {
    ledger(raw) == expected
}

pub fn verify_journal(raw: &[u8], expected: &str) -> bool {
    journal(raw) == expected
}

pub fn canonical(raw: &[u8]) -> Result<String, std::str::Utf8Error> {
    canonical_sha256(raw)
}

pub fn verify_canonical(raw: &[u8], expected: &str) -> Result<bool, std::str::Utf8Error> {
    Ok(canonical(raw)? == expected)
}

pub fn structured(value: &Value) -> Result<String, serde_json::Error> {
    canonical_json_sha256(value)
}

pub fn discussion_session(session: &work_model::discussion::DiscussionSession) -> String {
    let mut value = serde_json::to_value(session).expect("Session serializes");
    value["commit"]
        .as_object_mut()
        .expect("commit object")
        .remove("content_sha256");
    structured(&value).expect("JSON serializes")
}

pub fn discussion_operation(value: &Value) -> String {
    structured(value).expect("JSON serializes")
}

/// Bind semantic review to current context, decisions and all transitive upstream plans.
/// Review fields are excluded so that re-review does not recursively change the binding.
pub fn discussion_planning(
    session: &work_model::discussion::DiscussionSession,
    task_id: &str,
) -> String {
    use std::collections::BTreeSet;
    let mut ids = BTreeSet::from([task_id.to_owned()]);
    loop {
        let before = ids.len();
        for task in &session.tasks {
            if ids.contains(&task.id) {
                ids.extend(task.dependencies.iter().cloned());
            }
        }
        if before == ids.len() {
            break;
        }
    }
    let mut tasks: Vec<_> = session
        .tasks
        .iter()
        .filter(|t| ids.contains(&t.id))
        .cloned()
        .collect();
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    for task in &mut tasks {
        task.review = work_model::common::Nullable::Null;
    }
    let mut decisions: Vec<_> = session
        .decisions
        .iter()
        .filter(|d| {
            d.task_ids.iter().any(|id| ids.contains(id))
                || tasks.iter().any(|t| t.decision_ids.contains(&d.id))
                || tasks
                    .iter()
                    .flat_map(|t| &t.steps)
                    .flat_map(|s| &s.references)
                    .any(|r| {
                        r.kind == work_model::task::candidate::CandidateReferenceKind::Decisions
                            && (r.key == d.id || r.key == d.id.to_ascii_lowercase())
                    })
        })
        .collect();
    loop {
        let ids: std::collections::BTreeSet<_> = decisions
            .iter()
            .flat_map(|d| d.dependencies.iter())
            .collect();
        let next: Vec<_> = session
            .decisions
            .iter()
            .filter(|d| ids.contains(&d.id) && !decisions.iter().any(|old| old.id == d.id))
            .collect();
        if next.is_empty() {
            break;
        }
        decisions.extend(next);
    }
    decisions.sort_by(|a, b| a.id.cmp(&b.id));
    structured(
        &json!({"requirement_id":session.requirement_id,"context":session.context,
        "tasks":tasks,"decisions":decisions}),
    )
    .expect("review binding serializes")
}

pub fn discussion_approval(session_sha256: &str, approval_bytes: &[u8]) -> String {
    let mut bytes = b"WORK-DISCUSSION-PUBLICATION-V1\n".to_vec();
    bytes.extend_from_slice(session_sha256.as_bytes());
    bytes.push(b'\n');
    bytes.extend_from_slice(approval_bytes);
    raw(&bytes)
}

pub fn verify_structured(value: &Value, expected: &str) -> Result<bool, serde_json::Error> {
    Ok(structured(value)? == expected)
}

pub fn instruction_selection(scope: &str, sources: &[InstructionSource<'_>]) -> String {
    instructions_sha256(scope, sources)
}

pub fn hierarchy_selection(
    decision: &str,
    selected_paths: &[String],
    entries: &[Value],
    catalog_sha256: &str,
) -> String {
    crate::hierarchy::selection_sha256(decision, selected_paths, entries, catalog_sha256)
}

pub fn task_collection(index_sha256: &str, references: &[TaskItemReference]) -> String {
    collection_fingerprint_sha256(index_sha256, references)
}

pub fn task_provenance(source: &work_model::task::source::TaskProvenance) -> String {
    structured(&serde_json::to_value(source).expect("provenance serializes"))
        .expect("JSON serializes")
}
pub fn migration_source_approval(
    requirement: &str,
    sources: &[work_model::task::source::MigrationSourceEvidence],
) -> String {
    let material =
        serde_json::to_vec(&serde_json::json!({"requirement_id":requirement,"sources":sources}))
            .expect("evidence serializes");
    let mut bytes = b"WORK-MIGRATION-SOURCE-APPROVAL-V1\n".to_vec();
    bytes.extend(material);
    raw(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_raw_hash_preserves_bytes_short_reads_and_io_failures() {
        struct ShortReads {
            bytes: std::io::Cursor<Vec<u8>>,
            interrupted: bool,
        }
        impl std::io::Read for ShortReads {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if !self.interrupted {
                    self.interrupted = true;
                    return Err(std::io::ErrorKind::Interrupted.into());
                }
                let limit = buffer.len().min(37);
                std::io::Read::read(&mut self.bytes, &mut buffer[..limit])
            }
        }
        let bytes: Vec<_> = (0..131079).map(|index| (index % 251) as u8).collect();
        assert_eq!(
            raw_reader(ShortReads {
                bytes: std::io::Cursor::new(bytes.clone()),
                interrupted: false
            })
            .unwrap(),
            (bytes.len() as u64, raw(&bytes))
        );
        assert_eq!(
            raw_reader(&b"abc"[..]).unwrap().1,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        struct Failed;
        impl std::io::Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::PermissionDenied.into())
            }
        }
        assert_eq!(
            raw_reader(Failed).unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn facade_preserves_raw_canonical_and_structured_hashes() {
        let bytes = b"first\r\nsecond\r\n";
        let raw_digest = sha256_hex(bytes);
        let canonical_digest = canonical_sha256(bytes).unwrap();
        let value = json!({"z": [2, 1], "a": "value"});
        let structured_digest = canonical_json_sha256(&value).unwrap();

        assert_eq!(raw(bytes), raw_digest);
        assert!(verify_raw(bytes, &raw_digest));
        assert!(!verify_raw(b"changed", &raw_digest));
        assert_eq!(canonical(bytes).unwrap(), canonical_digest);
        assert!(verify_canonical(bytes, &canonical_digest).unwrap());
        assert!(!verify_canonical(b"changed", &canonical_digest).unwrap());
        assert_eq!(structured(&value).unwrap(), structured_digest);
        assert!(verify_structured(&value, &structured_digest).unwrap());
        assert!(!verify_structured(&json!({"a": "changed"}), &structured_digest).unwrap());
        assert!(canonical(b"\xff").is_err());
        assert!(verify_canonical(b"\xff", &canonical_digest).is_err());
    }

    #[test]
    fn task_collection_preserves_framing_and_reference_order() {
        let first = TaskItemReference {
            id: "TASK-001".into(),
            path: "tasks/TASK-001.json".into(),
            canonical_sha256: "a".repeat(64),
        };
        let second = TaskItemReference {
            id: "TASK-002".into(),
            path: "tasks/TASK-002.json".into(),
            canonical_sha256: "b".repeat(64),
        };
        let index = "c".repeat(64);
        assert_eq!(
            task_collection(&index, &[first.clone(), second.clone()]),
            "36f7a8d7b942cd29739fa1950700271e32f7bc1edba9cf8adf33d2f39afa96f7"
        );
        assert_ne!(
            task_collection(&index, &[first.clone(), second.clone()]),
            task_collection(&index, &[second, first])
        );
    }

    #[test]
    fn hierarchy_selection_preserves_catalog_and_entry_order() {
        let paths = vec!["web".to_owned(), "web/backend".to_owned()];
        let entries = vec![json!({"path": "web"}), json!({"path": "web/backend"})];
        let catalog = "a".repeat(64);
        let expected = crate::hierarchy::selection_sha256("guided", &paths, &entries, &catalog);
        assert_eq!(
            hierarchy_selection("guided", &paths, &entries, &catalog),
            expected
        );
        assert_ne!(
            hierarchy_selection("guided", &paths, &entries, &catalog),
            hierarchy_selection("guided", &paths, &entries, &"b".repeat(64))
        );
        let mut reversed = entries;
        reversed.reverse();
        assert_ne!(
            hierarchy_selection("guided", &paths, &reversed, &catalog),
            expected
        );
    }
}
