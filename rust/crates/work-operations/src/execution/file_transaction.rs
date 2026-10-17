//! Resolve current Task file inputs without granting execution capabilities.
use super::{ExecutionIssue, issue};
use serde_json::{Value, json};

// Shared pure history rules; storage supplies integrity-verified evidence.
pub mod history {
    use std::collections::{BTreeMap, BTreeSet};
    use work_model::execution::file_transaction::{FileBaselineConfirmation, FileState};
    use work_model::runtime::RuntimePhase;

    pub struct Entry<'a> {
        pub identity: &'a str,
        pub previous: Option<&'a str>,
        pub confirmation: Option<&'a FileBaselineConfirmation>,
        pub path_identities: &'a BTreeMap<String, Option<String>>,
        pub effective_identities: &'a BTreeMap<String, Option<String>>,
        pub phase: RuntimePhase,
        pub before: &'a BTreeMap<String, Option<FileState>>,
        pub after: &'a BTreeMap<String, Option<FileState>>,
    }

    #[derive(Debug, PartialEq, Eq)]
    pub struct Current {
        pub head: String,
        pub initial: BTreeMap<String, Option<FileState>>,
        pub expected: BTreeMap<String, Option<FileState>>,
        pub published_paths: BTreeSet<String>,
        pub path_identities: BTreeMap<String, Option<String>>,
    }

    /// Inputs are snapshots from integrity-verified transactions of one Task/Attempt.
    /// Order comes from predecessor identities, never directory order or timestamps.
    pub fn current(entries: &[Entry<'_>]) -> Result<Current, &'static str> {
        let mut by_id = BTreeMap::new();
        let mut successors = BTreeMap::new();
        let mut aliases = BTreeMap::new();
        for entry in entries {
            if entry.identity.is_empty() || by_id.insert(entry.identity, entry).is_some() {
                return Err("file_transaction_history_duplicate");
            }
            if successors.insert(entry.previous, entry.identity).is_some() {
                return Err("file_transaction_history_fork");
            }
            if !matches!(
                entry.phase,
                RuntimePhase::PublishedVerified | RuntimePhase::Restored
            ) {
                return Err("file_transaction_recovery_required");
            }
            if entry.before.is_empty() || !entry.before.keys().eq(entry.after.keys()) {
                return Err("file_transaction_incomplete_set");
            }
            for path in entry.before.keys() {
                if !crate::derivation::identity::runtime_relative_path(path)
                    || aliases
                        .insert(crate::canonical::portable_path_identity(path), path)
                        .is_some_and(|other| other != path)
                {
                    return Err("file_transaction_path_alias");
                }
            }
        }
        let mut result = Current {
            head: String::new(),
            initial: BTreeMap::new(),
            expected: BTreeMap::new(),
            published_paths: BTreeSet::new(),
            path_identities: BTreeMap::new(),
        };
        let mut visited = BTreeSet::new();
        let mut previous = None;
        while let Some(id) = successors.get(&previous) {
            if !visited.insert(*id) {
                return Err("file_transaction_history_cycle");
            }
            let entry = by_id[id];
            if let Some(confirmation) = entry.confirmation {
                if entry.previous != Some(confirmation.previous_transaction.as_str())
                    || confirmation.before_sha256
                        != before_sha256(entry.before, entry.path_identities)
                    || confirmation.evidence.trim().is_empty()
                {
                    return Err("file_transaction_baseline_confirmation_invalid");
                }
            }
            for (path, state) in entry.before {
                let state_changed = result
                    .expected
                    .get(path)
                    .is_some_and(|expected| expected != state);
                let identity_changed = result
                    .path_identities
                    .get(path)
                    .is_some_and(|expected| entry.path_identities.get(path) != Some(expected));
                if (state_changed || identity_changed) && entry.confirmation.is_none() {
                    return Err("file_transaction_history_baseline_mismatch");
                }
                result
                    .initial
                    .entry(path.clone())
                    .or_insert_with(|| state.clone());
            }
            // A restored later change returns to that change's accepted current baseline.
            // It never reverts the first successful change or invents a publication.
            let effective = if entry.phase == RuntimePhase::PublishedVerified {
                result.published_paths.extend(entry.after.keys().cloned());
                entry.after
            } else {
                entry.before
            };
            result.expected.extend(effective.clone());
            result
                .path_identities
                .extend(entry.effective_identities.clone());
            result.head = (*id).into();
            previous = Some(*id);
        }
        if visited.len() != entries.len() {
            return Err("file_transaction_history_disconnected");
        }
        Ok(result)
    }

    pub fn before_sha256(
        before: &BTreeMap<String, Option<FileState>>,
        path_identities: &BTreeMap<String, Option<String>>,
    ) -> String {
        crate::derivation::fingerprint::structured(
            &serde_json::json!({"files":before,"path_identities":path_identities}),
        )
        .expect("file snapshots serialize")
    }

    pub fn version(current: &Current) -> String {
        crate::derivation::fingerprint::structured(&serde_json::json!({
            "domain":"WORK-FILE-VERSION-V1", "head":current.head,
            "expected":current.expected, "path_identities":current.path_identities
        }))
        .expect("file version serializes")
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use work_model::execution::file_transaction::FileMetadata;
        fn state(bytes: &[u8]) -> Option<FileState> {
            Some(FileState {
                bytes: bytes.into(),
                metadata: FileMetadata {
                    readonly: false,
                    unix_mode: Some(0o100644),
                    unix_uid: Some(501),
                    unix_gid: Some(20),
                    windows_security: None,
                    macos_xattrs: Some(BTreeMap::new()),
                },
            })
        }
        fn tree(bytes: &[u8]) -> BTreeMap<String, Option<FileState>> {
            BTreeMap::from([("src/a".into(), state(bytes))])
        }
        #[test]
        fn later_success_and_restoration_use_their_own_baselines_in_predecessor_order() {
            let initial = tree(b"dirty user contents");
            let first = tree(b"first success");
            let second = tree(b"second success");
            let failed = tree(b"failed third write");
            let entries = [
                Entry {
                    identity: "third",
                    previous: Some("second"),
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::Restored,
                    before: &second,
                    after: &failed,
                },
                Entry {
                    identity: "first",
                    previous: None,
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::PublishedVerified,
                    before: &initial,
                    after: &first,
                },
                Entry {
                    identity: "second",
                    previous: Some("first"),
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::PublishedVerified,
                    before: &first,
                    after: &second,
                },
            ];
            let result = current(&entries).unwrap();
            assert_eq!(result.initial, initial);
            assert_eq!(result.expected, second);
            assert_eq!(result.head, "third");
            assert_eq!(result.published_paths, BTreeSet::from(["src/a".into()]));
        }
        #[test]
        fn incomplete_forked_disconnected_duplicate_and_unknown_histories_cannot_complete() {
            let before = tree(b"before");
            let after = tree(b"after");
            for mode in 0..5 {
                let mut entries = [
                    Entry {
                        identity: "first",
                        previous: None,
                        confirmation: None,
                        path_identities: &BTreeMap::new(),
                        effective_identities: &BTreeMap::new(),
                        phase: RuntimePhase::PublishedVerified,
                        before: &before,
                        after: &after,
                    },
                    Entry {
                        identity: "second",
                        previous: Some("first"),
                        confirmation: None,
                        path_identities: &BTreeMap::new(),
                        effective_identities: &BTreeMap::new(),
                        phase: RuntimePhase::Restored,
                        before: &after,
                        after: &before,
                    },
                ];
                match mode {
                    0 => entries[1].previous = None,
                    1 => entries[1].previous = Some("missing"),
                    2 => entries[1].identity = "first",
                    3 => entries[0].previous = Some("second"),
                    _ => entries[1].phase = RuntimePhase::Publishing,
                }
                assert!(current(&entries).is_err(), "mode {mode}");
            }
            let empty = BTreeMap::new();
            assert!(
                current(&[Entry {
                    identity: "first",
                    previous: None,
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::PublishedVerified,
                    before: &before,
                    after: &empty
                }])
                .is_err()
            );
            assert!(
                current(&[Entry {
                    identity: "first",
                    previous: None,
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::Restored,
                    before: &before,
                    after: &after,
                }])
                .unwrap()
                .published_paths
                .is_empty()
            );
        }
        #[test]
        fn all_paths_retain_latest_state_and_a_restored_create_is_not_a_success() {
            let before = tree(b"user");
            let after = tree(b"accepted");
            let missing = BTreeMap::from([("src/new".into(), None)]);
            let created = BTreeMap::from([("src/new".into(), state(b"failed create"))]);
            let result = current(&[
                Entry {
                    identity: "first",
                    previous: None,
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::PublishedVerified,
                    before: &before,
                    after: &after,
                },
                Entry {
                    identity: "second",
                    previous: Some("first"),
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::Restored,
                    before: &missing,
                    after: &created,
                },
            ])
            .unwrap();
            assert_eq!(result.expected["src/a"], after["src/a"]);
            assert_eq!(result.expected["src/new"], None);
            assert!(!result.published_paths.contains("src/new"));
            let alias = BTreeMap::from([("src/A".into(), state(b"alias"))]);
            assert!(
                current(&[
                    Entry {
                        identity: "first",
                        previous: None,
                        confirmation: None,
                        path_identities: &BTreeMap::new(),
                        effective_identities: &BTreeMap::new(),
                        phase: RuntimePhase::PublishedVerified,
                        before: &before,
                        after: &after
                    },
                    Entry {
                        identity: "second",
                        previous: Some("first"),
                        confirmation: None,
                        path_identities: &BTreeMap::new(),
                        effective_identities: &BTreeMap::new(),
                        phase: RuntimePhase::PublishedVerified,
                        before: &alias,
                        after: &alias
                    },
                ])
                .is_err()
            );
        }

        #[test]
        fn predecessor_identity_does_not_authorize_a_different_before_state() {
            let before = tree(b"user contents");
            let accepted = tree(b"first success");
            let foreign = tree(b"unrecorded edit");
            let proposed = tree(b"second change");
            for phase in [RuntimePhase::PublishedVerified, RuntimePhase::Restored] {
                assert_eq!(
                    current(&[
                        Entry {
                            identity: "first",
                            previous: None,
                            confirmation: None,
                            path_identities: &BTreeMap::new(),
                            effective_identities: &BTreeMap::new(),
                            phase: RuntimePhase::PublishedVerified,
                            before: &before,
                            after: &accepted,
                        },
                        Entry {
                            identity: "second",
                            previous: Some("first"),
                            confirmation: None,
                            path_identities: &BTreeMap::new(),
                            effective_identities: &BTreeMap::new(),
                            phase,
                            before: &foreign,
                            after: &proposed,
                        },
                    ]),
                    Err("file_transaction_history_baseline_mismatch"),
                    "a valid chain cannot conceal an unrecorded edit, even after restoration"
                );
            }
            let mut changed_metadata = accepted.clone();
            changed_metadata
                .get_mut("src/a")
                .unwrap()
                .as_mut()
                .unwrap()
                .metadata
                .unix_mode = Some(0o100600);
            let absent = BTreeMap::from([("src/a".into(), None)]);
            for incompatible in [&changed_metadata, &absent] {
                assert_eq!(
                    current(&[
                        Entry {
                            identity: "first",
                            previous: None,
                            confirmation: None,
                            path_identities: &BTreeMap::new(),
                            effective_identities: &BTreeMap::new(),
                            phase: RuntimePhase::PublishedVerified,
                            before: &before,
                            after: &accepted,
                        },
                        Entry {
                            identity: "second",
                            previous: Some("first"),
                            confirmation: None,
                            path_identities: &BTreeMap::new(),
                            effective_identities: &BTreeMap::new(),
                            phase: RuntimePhase::PublishedVerified,
                            before: incompatible,
                            after: &proposed,
                        },
                    ]),
                    Err("file_transaction_history_baseline_mismatch")
                );
            }
        }

        #[test]
        fn restored_change_leaves_the_preceding_version_as_the_next_baseline() {
            let before = tree(b"user contents");
            let accepted = tree(b"first success");
            let failed = tree(b"failed second change");
            let corrected = tree(b"third success");
            let result = current(&[
                Entry {
                    identity: "first",
                    previous: None,
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::PublishedVerified,
                    before: &before,
                    after: &accepted,
                },
                Entry {
                    identity: "second",
                    previous: Some("first"),
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::Restored,
                    before: &accepted,
                    after: &failed,
                },
                Entry {
                    identity: "third",
                    previous: Some("second"),
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::PublishedVerified,
                    before: &accepted,
                    after: &corrected,
                },
            ])
            .unwrap();
            assert_eq!(result.initial, before);
            assert_eq!(result.expected, corrected);
            assert_eq!(result.head, "third");
        }
        #[test]
        fn confirmed_baselines_bind_the_complete_before_and_predecessor() {
            let before = tree(b"original user contents");
            let accepted = tree(b"first success");
            let user = tree(b"confirmed user edit");
            let after = tree(b"next success");
            let confirmation = FileBaselineConfirmation {
                previous_transaction: "first".into(),
                before_sha256: before_sha256(&user, &BTreeMap::new()),
                evidence: "User confirmed the inspected bytes and metadata".into(),
            };
            for phase in [RuntimePhase::PublishedVerified, RuntimePhase::Restored] {
                for mode in 0..5 {
                    let mut proof = confirmation.clone();
                    let mut snapshot = user.clone();
                    match mode {
                        1 => proof.previous_transaction = "another".into(),
                        2 => proof.before_sha256 = before_sha256(&accepted, &BTreeMap::new()),
                        3 => proof.evidence = " ".into(),
                        4 => {
                            snapshot
                                .get_mut("src/a")
                                .unwrap()
                                .as_mut()
                                .unwrap()
                                .metadata
                                .unix_mode = Some(0o100600)
                        }
                        _ => {}
                    }
                    let result = current(&[
                        Entry {
                            identity: "first",
                            previous: None,
                            confirmation: None,
                            path_identities: &BTreeMap::new(),
                            effective_identities: &BTreeMap::new(),
                            phase: RuntimePhase::PublishedVerified,
                            before: &before,
                            after: &accepted,
                        },
                        Entry {
                            identity: "second",
                            previous: Some("first"),
                            confirmation: Some(&proof),
                            path_identities: &BTreeMap::new(),
                            effective_identities: &BTreeMap::new(),
                            phase,
                            before: &snapshot,
                            after: &after,
                        },
                    ]);
                    if mode == 0 {
                        let result = result.unwrap();
                        assert_eq!(result.initial, before);
                        assert_eq!(
                            result.expected,
                            if phase == RuntimePhase::Restored {
                                user.clone()
                            } else {
                                after.clone()
                            }
                        );
                    } else {
                        assert!(result.is_err(), "mode {mode}");
                    }
                }
            }
        }

        #[test]
        fn restored_writes_and_confirmed_user_baselines_invalidate_old_versions() {
            let before = tree(b"before");
            let after = tree(b"after");
            let first = Entry {
                identity: "first",
                previous: None,
                confirmation: None,
                path_identities: &BTreeMap::new(),
                effective_identities: &BTreeMap::new(),
                phase: RuntimePhase::PublishedVerified,
                before: &before,
                after: &after,
            };
            let version_before = version(&current(&[first]).unwrap());
            let result = current(&[
                Entry {
                    identity: "first",
                    previous: None,
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::PublishedVerified,
                    before: &before,
                    after: &after,
                },
                Entry {
                    identity: "second",
                    previous: Some("first"),
                    confirmation: None,
                    path_identities: &BTreeMap::new(),
                    effective_identities: &BTreeMap::new(),
                    phase: RuntimePhase::Restored,
                    before: &after,
                    after: &before,
                },
            ])
            .unwrap();
            assert_eq!(result.expected, after);
            assert_ne!(version_before, version(&result));
        }
    }
}

pub fn input_paths(collection: &Value, task: &Value) -> Result<Vec<String>, ExecutionIssue> {
    let mut paths = std::collections::BTreeSet::new();
    for input in task["inputs"].as_array().into_iter().flatten() {
        let path = if input["kind"] == "project_state" {
            input["source"].as_str()
        } else if input["kind"] == "task_output" {
            let (producer, file) = input["source"]
                .as_str()
                .and_then(|s| s.split_once('/'))
                .ok_or_else(|| {
                    issue(
                        "file_transaction_input_unverified",
                        "A file input must have a verified source.",
                        json!({}),
                    )
                })?;
            collection["tasks"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|t| t["id"] == producer)
                .and_then(|t| t["files"].as_array())
                .and_then(|fs| fs.iter().find(|f| f["id"] == file))
                .and_then(|f| f["path"].as_str().or_else(|| f["destination"].as_str()))
        } else {
            continue;
        };
        let path = path
            .filter(|p| crate::derivation::identity::runtime_relative_path(p))
            .ok_or_else(|| {
                issue(
                    "file_transaction_input_unverified",
                    "A file input must have a verified source.",
                    json!({}),
                )
            })?;
        paths.insert(path.to_owned());
    }
    Ok(paths.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_inputs_require_exact_current_sources_and_move_destinations() {
        let collection = json!({"tasks":[{"id":"TASK-001","files":[
            {"id":"FILE-001","action":"move","source":"old","destination":"src/current"}]}]});
        let task = json!({"inputs":[{"kind":"project_state","source":"src/local"},
            {"kind":"task_output","source":"TASK-001/FILE-001"}]});
        assert_eq!(
            input_paths(&collection, &task).unwrap(),
            ["src/current", "src/local"]
        );
        for source in ["TASK-001/FILE-999", "TASK-999/FILE-001", "../secret"] {
            let task = json!({"inputs":[{"kind":"task_output","source":source}]});
            assert!(input_paths(&collection, &task).is_err());
        }
    }
}
