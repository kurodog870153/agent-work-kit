//! Runtime host metadata shared by Work and Tooling.

use serde::{Deserialize, Serialize};

/// Observed runtime metadata. This records inheritance, not a sandbox attestation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostEnvironment {
    pub mode: HostExecutionMode,
    pub os: String,
    pub architecture: String,
    pub rust_workspace_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostExecutionMode {
    Inherited,
}

impl HostEnvironment {
    pub fn inherited() -> Self {
        Self {
            mode: HostExecutionMode::Inherited,
            os: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            rust_workspace_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionEnvironment {
    pub host: HostEnvironment,
}

impl ExecutionEnvironment {
    pub fn inherited() -> Self {
        Self {
            host: HostEnvironment::inherited(),
        }
    }
}
