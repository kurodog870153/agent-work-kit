use std::collections::HashSet;
use std::path::PathBuf;

use serde::Deserialize;
use tooling_feature::operation_evaluation::{Evaluation, OperationEvidence};
use tooling_feature::policy_freshness::ReviewState;
use tooling_infrastructure::host_binding::{ObservedTool, OperationNeed};
use tooling_model::HostSemanticReview;
use tooling_operations::operation_outcome::{
    AlternativeReview, NextAction, Outcome, Reconciliation,
};
use tooling_operations::operation_safety::{AuthorizedEffects, ObservedEffect};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewInput {
    Valid,
    Invalid,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticReviewInput {
    pub policy_reviewed: bool,
    pub policy_compatible: bool,
    pub fitness_reviewed: bool,
    pub tool_fit: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedToolInput {
    pub name: String,
    pub interfaces: Vec<String>,
    pub capabilities: Vec<String>,
    pub supported_operations: Vec<String>,
    pub project_root: PathBuf,
    pub session: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationNeedInput {
    pub operation: String,
    pub interface: String,
    pub capability: String,
    pub project_root: PathBuf,
    pub target: PathBuf,
    pub session: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EffectInput {
    Read,
    Write,
    ExternalAccess {
        recipient: String,
        transmits_data: bool,
    },
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizationInput {
    pub writes: bool,
    pub external_recipients: Vec<String>,
    pub data_recipients: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeInput {
    Complete,
    Failed,
    Partial,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReconciliationInput {
    pub effects_inspected: bool,
    pub unresolved_effects: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlternativeInput {
    pub policy_valid: bool,
    pub binding_valid: bool,
    pub evidence_fresh: bool,
    pub authorized: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluateRequest {
    pub policy_state: ReviewInput,
    pub semantic_review: SemanticReviewInput,
    pub observed_tools: Vec<ObservedToolInput>,
    pub need: OperationNeedInput,
    pub effects: Vec<EffectInput>,
    pub authorization: AuthorizationInput,
    #[serde(default)]
    pub outcome: Option<OutcomeInput>,
    #[serde(default)]
    pub reconciliation: Option<ReconciliationInput>,
    #[serde(default)]
    pub alternative: Option<AlternativeInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assessment {
    pub decision: &'static str,
    pub reason_code: &'static str,
}

pub fn evaluate(request: EvaluateRequest) -> Result<Assessment, &'static str> {
    if request.outcome.is_none()
        && (request.reconciliation.is_some() || request.alternative.is_some())
    {
        return Err("outcome_context_invalid");
    }
    let observed: Vec<_> = request
        .observed_tools
        .into_iter()
        .map(|tool| ObservedTool {
            name: tool.name,
            interfaces: tool.interfaces.into_iter().collect::<HashSet<_>>(),
            capabilities: tool.capabilities.into_iter().collect::<HashSet<_>>(),
            supported_operations: tool
                .supported_operations
                .into_iter()
                .collect::<HashSet<_>>(),
            project_root: tool.project_root,
            session: tool.session,
        })
        .collect();
    let need = OperationNeed {
        operation: request.need.operation,
        interface: request.need.interface,
        capability: request.need.capability,
        project_root: request.need.project_root,
        target: request.need.target,
        session: request.need.session,
    };
    let effects: Vec<_> = request
        .effects
        .into_iter()
        .map(|effect| match effect {
            EffectInput::Read => ObservedEffect::Read,
            EffectInput::Write => ObservedEffect::Write,
            EffectInput::Unknown => ObservedEffect::Unknown,
            EffectInput::ExternalAccess {
                recipient,
                transmits_data,
            } => ObservedEffect::ExternalAccess {
                recipient,
                transmits_data,
            },
        })
        .collect();
    let authorization = AuthorizedEffects {
        writes: request.authorization.writes,
        external_recipients: request
            .authorization
            .external_recipients
            .into_iter()
            .collect(),
        data_recipients: request.authorization.data_recipients.into_iter().collect(),
    };
    let policy_state = match request.policy_state {
        ReviewInput::Valid => ReviewState::Valid,
        ReviewInput::Invalid => ReviewState::Invalid,
        ReviewInput::Unknown => ReviewState::Unknown,
    };
    let evidence = OperationEvidence {
        policy_state,
        semantic_review: HostSemanticReview {
            policy_reviewed: request.semantic_review.policy_reviewed,
            policy_compatible: request.semantic_review.policy_compatible,
            fitness_reviewed: request.semantic_review.fitness_reviewed,
            tool_fit: request.semantic_review.tool_fit,
        },
        observed_tools: &observed,
        need: &need,
        effects: &effects,
        authorization: &authorization,
    };
    match tooling_feature::operation_evaluation::evaluate(&evidence) {
        Evaluation::Inspect(reason_code) => {
            return Ok(Assessment {
                decision: "needs_inspection",
                reason_code,
            });
        }
        Evaluation::Stop(reason_code) => {
            return Ok(Assessment {
                decision: "stop",
                reason_code,
            });
        }
        Evaluation::Continue => {}
    }
    let Some(outcome) = request.outcome else {
        return Ok(Assessment {
            decision: "continue",
            reason_code: "operation_ready",
        });
    };
    let outcome = match outcome {
        OutcomeInput::Complete => Outcome::Complete,
        OutcomeInput::Failed => Outcome::Failed,
        OutcomeInput::Partial => Outcome::Partial,
        OutcomeInput::Unknown => Outcome::Unknown,
    };
    let reconciliation = request
        .reconciliation
        .map(|value| Reconciliation {
            effects_inspected: value.effects_inspected,
            unresolved_effects: value.unresolved_effects,
        })
        .unwrap_or(Reconciliation {
            effects_inspected: false,
            unresolved_effects: true,
        });
    let alternative = request.alternative.map(|value| AlternativeReview {
        policy_valid: value.policy_valid,
        binding_valid: value.binding_valid,
        evidence_fresh: value.evidence_fresh,
        authorized: value.authorized,
    });
    let assessment = match tooling_operations::operation_outcome::next_action(
        outcome,
        reconciliation,
        alternative,
    ) {
        NextAction::ReportComplete => Assessment {
            decision: "continue",
            reason_code: "operation_complete",
        },
        NextAction::InspectEffects => Assessment {
            decision: "needs_inspection",
            reason_code: "effects_inspection_required",
        },
        NextAction::ResolveEffects => Assessment {
            decision: "stop",
            reason_code: "effects_unresolved",
        },
        NextAction::Stop => Assessment {
            decision: "stop",
            reason_code: "alternative_not_ready",
        },
        NextAction::AlternativeAllowed => Assessment {
            decision: "continue",
            reason_code: "alternative_review_complete",
        },
    };
    Ok(assessment)
}
