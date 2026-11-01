use std::path::PathBuf;

use serde::Deserialize;
use tooling_infrastructure::host_binding::validate_target;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvaluateRequest {
    pub project_root: PathBuf,
    pub target: PathBuf,
}

pub struct Assessment {
    pub canonical_target: PathBuf,
}

pub fn evaluate(request: EvaluateRequest) -> Result<Assessment, &'static str> {
    let canonical_target = validate_target(&request.project_root, &request.target)
        .map_err(|_| "target_boundary_invalid")?;
    Ok(Assessment { canonical_target })
}
