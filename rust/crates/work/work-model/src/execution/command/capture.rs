//! Bounded actual command output and copies of required reports.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_STREAM_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REPORT_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_REPORT_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_VALIDATION_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandCapturePolicy {
    pub stream_limit_bytes: u64,
    pub report_limit_bytes: u64,
    pub report_total_limit_bytes: u64,
    pub report_paths: BTreeSet<String>,
}

impl Default for CommandCapturePolicy {
    fn default() -> Self {
        Self {
            stream_limit_bytes: MAX_STREAM_BYTES as u64,
            report_limit_bytes: MAX_REPORT_BYTES,
            report_total_limit_bytes: MAX_REPORT_TOTAL_BYTES,
            report_paths: BTreeSet::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedArtifact {
    pub file: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum CapturedReport {
    Available { artifact: CapturedArtifact },
    Unavailable { reason_code: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandCapturedEvidence {
    pub stdout: CapturedArtifact,
    pub stderr: CapturedArtifact,
    pub reports: BTreeMap<String, CapturedReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandExecutionEvidence {
    pub task_id: String,
    pub attempt_id: String,
    pub record_id: String,
    pub receipt_dir: String,
    pub approved_sha256: String,
    pub started_sha256: String,
    pub finished_sha256: String,
    pub exit_code: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationCommandEvidence {
    /// Current Attempt, or the explicitly authorized carried-record source Attempt.
    pub selected_from_attempt_id: String,
    pub execution: CommandExecutionEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationExecutionEvidence {
    pub task_id: String,
    pub attempt_id: String,
    pub record_id: String,
    pub engine_sha256: String,
    pub condition_sha256: String,
    pub passed: bool,
    pub commands: BTreeMap<String, ValidationCommandEvidence>,
}
