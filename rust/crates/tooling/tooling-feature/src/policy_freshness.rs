//! Track Host reviews affected by Tooling source, binding and evidence changes.

use std::collections::{BTreeMap, HashSet};

use crate::policy_composition::OperationPolicy;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewState {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationReview {
    pub policy: ReviewState,
    pub binding: ReviewState,
    pub evidence: ReviewState,
}

impl OperationReview {
    pub fn usable(self) -> bool {
        self.policy == ReviewState::Valid
            && self.binding == ReviewState::Valid
            && self.evidence == ReviewState::Valid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    Source,
    Binding,
    Evidence,
}

#[derive(Debug, Clone)]
struct TrackedOperation {
    sources: HashSet<String>,
    binding_keys: HashSet<String>,
    evidence_keys: HashSet<String>,
    review: OperationReview,
}

/// Host-provided keys identify real target/session bindings and evidence versions.
/// Tooling does not infer freshness from a tool name or treat unknown as valid.
#[derive(Debug, Default)]
pub struct OperationTracker {
    operations: BTreeMap<String, TrackedOperation>,
}

impl OperationTracker {
    pub fn track(
        &mut self,
        operation_id: String,
        policy: &OperationPolicy,
        binding_keys: impl IntoIterator<Item = String>,
        evidence_keys: impl IntoIterator<Item = String>,
        review: OperationReview,
    ) {
        self.operations.insert(
            operation_id,
            TrackedOperation {
                sources: policy
                    .sources()
                    .iter()
                    .map(|source| source.relative_path.clone())
                    .collect(),
                binding_keys: binding_keys.into_iter().collect(),
                evidence_keys: evidence_keys.into_iter().collect(),
                review,
            },
        );
    }

    /// Marks only dependent operations unknown; the Host must review before use.
    pub fn invalidate(&mut self, kind: ChangeKind, key: &str) -> Vec<String> {
        let mut affected = Vec::new();
        for (id, tracked) in &mut self.operations {
            let applies = match kind {
                ChangeKind::Source => tracked.sources.contains(key),
                ChangeKind::Binding => tracked.binding_keys.contains(key),
                ChangeKind::Evidence => tracked.evidence_keys.contains(key),
            };
            if applies {
                match kind {
                    ChangeKind::Source => tracked.review.policy = ReviewState::Unknown,
                    ChangeKind::Binding => tracked.review.binding = ReviewState::Unknown,
                    ChangeKind::Evidence => tracked.review.evidence = ReviewState::Unknown,
                }
                affected.push(id.clone());
            }
        }
        affected
    }

    pub fn review(&self, operation_id: &str) -> Option<OperationReview> {
        self.operations.get(operation_id).map(|entry| entry.review)
    }

    /// Accept only the Host's explicit new assessment for the affected facet.
    pub fn set_review(&mut self, operation_id: &str, kind: ChangeKind, state: ReviewState) -> bool {
        let Some(tracked) = self.operations.get_mut(operation_id) else {
            return false;
        };
        match kind {
            ChangeKind::Source => tracked.review.policy = state,
            ChangeKind::Binding => tracked.review.binding = state,
            ChangeKind::Evidence => tracked.review.evidence = state,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn node(path: &Path) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("instructions.md"),
            "---\nname: Policy\ndescription: Applicable policy.\nmetadata:\n  tags:\n    - policy\n---\nPolicy.\n",
        )
        .unwrap();
    }

    #[test]
    fn source_failure_isolated_to_dependents_and_root_affects_all() {
        let root = std::env::temp_dir().join(format!(
            "skill-freshness-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root);
        node(&root.join("interfaces"));
        node(&root.join("interfaces/mcp"));
        node(&root.join("families"));
        node(&root.join("families/serena"));
        let mut interface = OperationPolicy::default();
        interface
            .update(&root, &["interfaces/mcp"], |_, _, _| false, |_| Ok(()))
            .unwrap();
        let mut family = OperationPolicy::default();
        family
            .update(&root, &["families/serena"], |_, _, _| false, |_| Ok(()))
            .unwrap();
        let valid = OperationReview {
            policy: ReviewState::Valid,
            binding: ReviewState::Valid,
            evidence: ReviewState::Valid,
        };
        let mut tracker = OperationTracker::default();
        tracker.track(
            "mcp".into(),
            &interface,
            ["target:a".into(), "session:one".into()],
            ["index:a".into(), "analysis:a".into()],
            valid,
        );
        tracker.track(
            "serena".into(),
            &family,
            ["target:b".into(), "session:two".into()],
            ["workspace:b".into(), "result:b".into()],
            valid,
        );
        assert_eq!(
            tracker.invalidate(ChangeKind::Source, "interfaces/mcp/instructions.md"),
            ["mcp"]
        );
        assert!(!tracker.review("mcp").unwrap().usable());
        assert!(tracker.review("serena").unwrap().usable());
        assert_eq!(
            tracker.invalidate(ChangeKind::Source, "instructions.md"),
            ["mcp", "serena"]
        );
        assert!(!tracker.review("serena").unwrap().usable());
    }

    #[test]
    fn target_and_evidence_changes_need_separate_host_reviews() {
        let root = std::env::temp_dir().join(format!(
            "skill-freshness-facets-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        node(&root);
        let mut policy = OperationPolicy::default();
        policy
            .update(&root, &[], |_, _, _| false, |_| Ok(()))
            .unwrap();
        let mut tracker = OperationTracker::default();
        tracker.track(
            "analyze".into(),
            &policy,
            ["target:a".into(), "session:a".into()],
            [
                "index:a".into(),
                "analysis:a".into(),
                "workspace:a".into(),
                "result:a".into(),
            ],
            OperationReview {
                policy: ReviewState::Valid,
                binding: ReviewState::Valid,
                evidence: ReviewState::Valid,
            },
        );
        assert!(
            tracker
                .invalidate(ChangeKind::Evidence, "index:b")
                .is_empty()
        );
        assert_eq!(
            tracker.invalidate(ChangeKind::Binding, "session:a"),
            ["analyze"]
        );
        assert!(!tracker.review("analyze").unwrap().usable());
        assert!(tracker.set_review("analyze", ChangeKind::Binding, ReviewState::Valid));
        for key in ["target:a", "session:a"] {
            assert_eq!(tracker.invalidate(ChangeKind::Binding, key), ["analyze"]);
            assert!(!tracker.review("analyze").unwrap().usable());
            assert!(tracker.set_review("analyze", ChangeKind::Binding, ReviewState::Valid));
        }
        for key in ["index:a", "analysis:a", "workspace:a", "result:a"] {
            assert_eq!(tracker.invalidate(ChangeKind::Evidence, key), ["analyze"]);
            assert!(!tracker.review("analyze").unwrap().usable());
            assert!(tracker.set_review("analyze", ChangeKind::Evidence, ReviewState::Valid));
        }
        assert_eq!(
            tracker.invalidate(ChangeKind::Evidence, "result:a"),
            ["analyze"]
        );
        assert!(tracker.set_review("analyze", ChangeKind::Evidence, ReviewState::Invalid));
        assert!(!tracker.review("analyze").unwrap().usable());
        assert!(tracker.set_review("analyze", ChangeKind::Evidence, ReviewState::Valid));
        assert!(tracker.review("analyze").unwrap().usable());
    }
}
