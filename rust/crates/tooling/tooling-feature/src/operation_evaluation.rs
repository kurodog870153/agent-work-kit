//! Fail-closed assessment of pre-operation evidence supplied by the Host.

use crate::policy_freshness::ReviewState;
use tooling_infrastructure::host_binding::{BindingError, ObservedTool, OperationNeed, bind};
pub use tooling_model::HostSemanticReview;
use tooling_operations::operation_safety::{
    AuthorizedEffects, EffectError, ObservedEffect, check_effects,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evaluation {
    Continue,
    Inspect(&'static str),
    Stop(&'static str),
}

pub struct OperationEvidence<'a> {
    pub policy_state: ReviewState,
    pub semantic_review: HostSemanticReview,
    pub observed_tools: &'a [ObservedTool],
    pub need: &'a OperationNeed,
    pub effects: &'a [ObservedEffect],
    pub authorization: &'a AuthorizedEffects,
}

pub fn evaluate(evidence: &OperationEvidence<'_>) -> Evaluation {
    match evidence.policy_state {
        ReviewState::Invalid => return Evaluation::Stop("policy_invalid"),
        ReviewState::Unknown => return Evaluation::Inspect("policy_unverified"),
        ReviewState::Valid => {}
    }
    let review = evidence.semantic_review;
    if !review.policy_reviewed {
        return Evaluation::Inspect("policy_semantics_unreviewed");
    }
    if !review.policy_compatible {
        return Evaluation::Stop("policy_conflict");
    }
    if !review.fitness_reviewed {
        return Evaluation::Inspect("tool_fitness_unreviewed");
    }
    if !review.tool_fit {
        return Evaluation::Stop("tool_unfit");
    }
    if evidence.effects.is_empty() {
        return Evaluation::Inspect("effects_unobserved");
    }
    if let Err(failure) = bind(evidence.observed_tools, evidence.need) {
        return match failure {
            BindingError::NoTool => Evaluation::Inspect("tool_unobserved"),
            _ => Evaluation::Stop("tool_binding_invalid"),
        };
    }
    match check_effects(evidence.effects, evidence.authorization) {
        Ok(()) => Evaluation::Continue,
        Err(EffectError::Unknown) => Evaluation::Inspect("effects_unknown"),
        Err(_) => Evaluation::Stop("effects_unauthorized"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::fs;

    #[test]
    fn requires_observation_review_and_authorized_effects() {
        let root = std::env::temp_dir().join(format!(
            "tooling-evaluation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.rs"), "fn main() {}\n").unwrap();
        let need = OperationNeed {
            operation: "find-references".into(),
            interface: "mcp".into(),
            capability: "code-analysis".into(),
            project_root: root.clone(),
            target: root.join("source.rs"),
            session: None,
        };
        let observed = [ObservedTool {
            name: "analyzer".into(),
            interfaces: HashSet::from(["mcp".into()]),
            capabilities: HashSet::from(["code-analysis".into()]),
            supported_operations: HashSet::from(["find-references".into()]),
            project_root: root,
            session: None,
        }];
        let authorization = AuthorizedEffects::default();
        let mut evidence = OperationEvidence {
            policy_state: ReviewState::Valid,
            semantic_review: HostSemanticReview {
                policy_reviewed: true,
                policy_compatible: true,
                fitness_reviewed: true,
                tool_fit: true,
            },
            observed_tools: &observed,
            need: &need,
            effects: &[ObservedEffect::Read],
            authorization: &authorization,
        };
        assert_eq!(evaluate(&evidence), Evaluation::Continue);
        evidence.semantic_review.fitness_reviewed = false;
        assert_eq!(
            evaluate(&evidence),
            Evaluation::Inspect("tool_fitness_unreviewed")
        );
        evidence.semantic_review.fitness_reviewed = true;
        evidence.semantic_review.policy_compatible = false;
        assert_eq!(evaluate(&evidence), Evaluation::Stop("policy_conflict"));
        evidence.semantic_review.policy_compatible = true;
        evidence.effects = &[ObservedEffect::Write];
        assert_eq!(
            evaluate(&evidence),
            Evaluation::Stop("effects_unauthorized")
        );
        evidence.effects = &[ObservedEffect::Unknown];
        assert_eq!(evaluate(&evidence), Evaluation::Inspect("effects_unknown"));
        evidence.effects = &[];
        assert_eq!(
            evaluate(&evidence),
            Evaluation::Inspect("effects_unobserved")
        );
        evidence.effects = &[ObservedEffect::Read];
        evidence.observed_tools = &[];
        assert_eq!(evaluate(&evidence), Evaluation::Inspect("tool_unobserved"));
    }
}
