//! A complete Task change set reads one initial tree and produces one final tree.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub struct StateIssue {
    pub kind: &'static str,
    pub path: String,
}

pub fn final_state(
    files: &[Value],
    initial: &BTreeMap<String, bool>,
    identity: impl Fn(&str) -> String,
) -> Result<BTreeMap<String, bool>, StateIssue> {
    let failure = |kind, path: &str| StateIssue {
        kind,
        path: path.into(),
    };
    let mut moved = BTreeSet::new();
    let mut written = BTreeSet::new();
    for file in files {
        let destination = if file["action"] == "move" {
            let source = file["source"].as_str().unwrap_or("");
            if !moved.insert(identity(source))
                || identity(source) == identity(file["destination"].as_str().unwrap_or(""))
            {
                return Err(failure("move_source_duplicate", source));
            }
            file["destination"].as_str().unwrap_or("")
        } else {
            file["path"].as_str().unwrap_or("")
        };
        if !written.insert(identity(destination)) {
            return Err(failure("destination_duplicate", destination));
        }
    }
    for file in files {
        let path = file["path"].as_str().unwrap_or("");
        let exists = |path: &str| initial.get(&identity(path)).copied().unwrap_or(false);
        match file["action"].as_str() {
            Some("create") if exists(path) && !moved.contains(&identity(path)) => {
                return Err(failure("create_target_exists", path));
            }
            Some("modify") if !exists(path) => return Err(failure("modify_target_missing", path)),
            Some("move") => {
                let source = file["source"].as_str().unwrap_or("");
                let destination = file["destination"].as_str().unwrap_or("");
                if !exists(source) {
                    return Err(failure("move_source_missing", source));
                }
                if exists(destination) && !moved.contains(&identity(destination)) {
                    return Err(failure("move_destination_exists", destination));
                }
            }
            _ => {}
        }
    }
    let mut result = initial.clone();
    for path in moved {
        result.insert(path, false);
    }
    for path in written {
        result.insert(path, true);
    }
    Ok(result)
}
