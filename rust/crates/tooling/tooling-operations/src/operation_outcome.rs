//! Decide when a Host may consider an alternative after uncertain results.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Complete,
    Failed,
    Partial,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reconciliation {
    /// The Host checked actual local and external effects, not just a return message.
    pub effects_inspected: bool,
    pub unresolved_effects: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlternativeReview {
    pub policy_valid: bool,
    pub binding_valid: bool,
    pub evidence_fresh: bool,
    pub authorized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextAction {
    ReportComplete,
    InspectEffects,
    ResolveEffects,
    Stop,
    AlternativeAllowed,
}

pub fn next_action(
    outcome: Outcome,
    reconciliation: Reconciliation,
    alternative: Option<AlternativeReview>,
) -> NextAction {
    if outcome == Outcome::Complete {
        return NextAction::ReportComplete;
    }
    if !reconciliation.effects_inspected {
        return NextAction::InspectEffects;
    }
    if reconciliation.unresolved_effects {
        return NextAction::ResolveEffects;
    }
    if alternative.is_some_and(|review| {
        review.policy_valid && review.binding_valid && review.evidence_fresh && review.authorized
    }) {
        NextAction::AlternativeAllowed
    } else {
        NextAction::Stop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_and_unknown_results_require_actual_effect_review_before_fallback() {
        let approved = AlternativeReview {
            policy_valid: true,
            binding_valid: true,
            evidence_fresh: true,
            authorized: true,
        };
        let unchecked = Reconciliation {
            effects_inspected: false,
            unresolved_effects: false,
        };
        for outcome in [Outcome::Partial, Outcome::Unknown, Outcome::Failed] {
            assert_eq!(
                next_action(outcome, unchecked, Some(approved)),
                NextAction::InspectEffects
            );
            assert_eq!(
                next_action(
                    outcome,
                    Reconciliation {
                        effects_inspected: true,
                        unresolved_effects: true
                    },
                    Some(approved)
                ),
                NextAction::ResolveEffects
            );
        }
        assert_eq!(
            next_action(Outcome::Complete, unchecked, None),
            NextAction::ReportComplete
        );
    }

    #[test]
    fn fallback_requires_independent_policy_binding_evidence_and_authorization() {
        let inspected = Reconciliation {
            effects_inspected: true,
            unresolved_effects: false,
        };
        let approved = AlternativeReview {
            policy_valid: true,
            binding_valid: true,
            evidence_fresh: true,
            authorized: true,
        };
        assert_eq!(
            next_action(Outcome::Partial, inspected, Some(approved)),
            NextAction::AlternativeAllowed
        );
        for rejected in [
            AlternativeReview {
                policy_valid: false,
                ..approved
            },
            AlternativeReview {
                binding_valid: false,
                ..approved
            },
            AlternativeReview {
                evidence_fresh: false,
                ..approved
            },
            AlternativeReview {
                authorized: false,
                ..approved
            },
        ] {
            assert_eq!(
                next_action(Outcome::Partial, inspected, Some(rejected)),
                NextAction::Stop
            );
        }
        assert_eq!(
            next_action(Outcome::Unknown, inspected, None),
            NextAction::Stop
        );
    }
}
