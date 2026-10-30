//! Tooling evidence and review data shared across layers.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostSemanticReview {
    pub policy_reviewed: bool,
    pub policy_compatible: bool,
    pub fitness_reviewed: bool,
    pub tool_fit: bool,
}
