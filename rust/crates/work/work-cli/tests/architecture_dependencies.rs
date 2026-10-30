//! Enforce the six Work crate edges for normal, build, and dev dependencies.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use serde_json::Value;

const CRATES: [&str; 6] = [
    "work-cli",
    "work-feature",
    "work-flow",
    "work-infrastructure",
    "work-model",
    "work-operations",
];

fn allowed() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    BTreeMap::from([
        ("work-model", BTreeSet::new()),
        ("work-operations", BTreeSet::from(["work-model"])),
        (
            "work-feature",
            BTreeSet::from(["work-model", "work-operations"]),
        ),
        ("work-flow", BTreeSet::from(["work-feature", "work-model"])),
        (
            "work-infrastructure",
            BTreeSet::from(["work-feature", "work-model", "work-operations"]),
        ),
        (
            "work-cli",
            BTreeSet::from(["work-flow", "work-infrastructure", "work-model"]),
        ),
    ])
}

fn check_edge(source: &str, destination: &str, kind: &str) -> Result<(), String> {
    if !["normal", "build", "dev"].contains(&kind) {
        return Err(format!("unknown dependency kind: {kind}"));
    }
    let table = allowed();
    let Some(destinations) = table.get(source) else {
        return Err(format!("unknown Work source: {source}"));
    };
    if destinations.contains(destination) {
        Ok(())
    } else {
        Err(format!(
            "forbidden {kind} Work edge: {source} -> {destination}"
        ))
    }
}

#[test]
fn workspace_has_only_six_work_crates_and_allowed_direct_edges() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .env("RUSTUP_TOOLCHAIN", env!("CARGO_PKG_RUST_VERSION"))
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .expect("cargo metadata starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let members: BTreeSet<_> = metadata["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let packages = metadata["packages"].as_array().unwrap();
    let work_packages: Vec<_> = packages
        .iter()
        .filter(|package| {
            package["name"]
                .as_str()
                .is_some_and(|name| name.starts_with("work-"))
        })
        .collect();
    let actual: BTreeSet<_> = work_packages
        .iter()
        .filter(|package| members.contains(package["id"].as_str().unwrap()))
        .map(|package| package["name"].as_str().unwrap())
        .collect();
    assert_eq!(actual, BTreeSet::from(CRATES));
    for package in work_packages {
        let source = package["name"].as_str().unwrap();
        for dependency in package["dependencies"].as_array().unwrap() {
            let destination = dependency["name"].as_str().unwrap();
            if !destination.starts_with("work-") {
                continue;
            }
            let kind = dependency["kind"].as_str().unwrap_or("normal");
            check_edge(source, destination, kind).unwrap();
        }
    }
}

#[test]
fn every_forbidden_direction_is_rejected_for_all_dependency_kinds() {
    let table = allowed();
    for source in CRATES {
        for destination in CRATES {
            for kind in ["normal", "build", "dev"] {
                let result = check_edge(source, destination, kind);
                assert_eq!(
                    result.is_ok(),
                    table[source].contains(destination),
                    "{kind} {source} -> {destination}"
                );
            }
        }
    }
    assert!(check_edge("work-cli", "work-unlisted", "normal").is_err());
}

const SKILL_CRATES: [&str; 5] = [
    "skill-api",
    "skill-feature",
    "skill-infrastructure",
    "skill-model",
    "skill-operations",
];

const TOOLING_CRATES: [&str; 6] = [
    "tooling-cli",
    "tooling-feature",
    "tooling-flow",
    "tooling-infrastructure",
    "tooling-model",
    "tooling-operations",
];

fn domain_allowed(source: &str, destination: &str, kind: &str) -> bool {
    if !["normal", "build", "dev"].contains(&kind) {
        return false;
    }
    match source {
        "skill-api" => [
            "skill-model",
            "skill-operations",
            "skill-feature",
            "skill-infrastructure",
        ]
        .contains(&destination),
        "skill-infrastructure" => ["skill-model", "skill-operations"].contains(&destination),
        "skill-operations" => destination == "skill-model",
        "skill-model" | "skill-feature" => false,
        "tooling-model" | "tooling-operations" | "tooling-infrastructure" => false,
        "tooling-feature" => [
            "skill-api",
            "tooling-model",
            "tooling-operations",
            "tooling-infrastructure",
        ]
        .contains(&destination),
        "tooling-flow" => [
            "skill-api",
            "tooling-feature",
            "tooling-infrastructure",
            "tooling-operations",
            "tooling-model",
        ]
        .contains(&destination),
        "tooling-cli" => destination == "tooling-flow",
        work if work.starts_with("work-") => {
            destination == "skill-api"
                || (destination.starts_with("work-") && check_edge(work, destination, kind).is_ok())
        }
        _ => false,
    }
}

#[test]
fn domain_dependencies_follow_public_skill_entry_and_tooling_layers() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .env("RUSTUP_TOOLCHAIN", env!("CARGO_PKG_RUST_VERSION"))
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
        ])
        .arg(manifest)
        .output()
        .expect("cargo metadata starts");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let members: BTreeSet<_> = metadata["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    for (prefix, expected) in [
        ("skill-", SKILL_CRATES.as_slice()),
        ("tooling-", TOOLING_CRATES.as_slice()),
        ("work-", CRATES.as_slice()),
    ] {
        let actual: BTreeSet<_> = packages
            .iter()
            .filter(|package| members.contains(package["id"].as_str().unwrap()))
            .filter_map(|package| package["name"].as_str())
            .filter(|name| name.starts_with(prefix))
            .collect();
        assert_eq!(
            actual,
            expected.iter().copied().collect(),
            "{prefix} workspace members"
        );
    }
    for package in packages {
        let source = package["name"].as_str().unwrap();
        if !source.starts_with("skill-")
            && !source.starts_with("tooling-")
            && !source.starts_with("work-")
        {
            continue;
        }
        for dependency in package["dependencies"].as_array().unwrap() {
            let destination = dependency["name"].as_str().unwrap();
            if !destination.starts_with("skill-")
                && !destination.starts_with("tooling-")
                && !destination.starts_with("work-")
            {
                continue;
            }
            let kind = dependency["kind"].as_str().unwrap_or("normal");
            assert!(
                domain_allowed(source, destination, kind),
                "forbidden {kind} edge: {source} -> {destination}"
            );
        }
    }
}

#[test]
fn forbidden_cross_domain_edges_are_rejected_for_every_dependency_kind() {
    let sources = SKILL_CRATES.into_iter().chain(TOOLING_CRATES).chain(CRATES);
    let destinations = SKILL_CRATES
        .into_iter()
        .chain(TOOLING_CRATES)
        .chain(CRATES)
        .collect::<Vec<_>>();
    for source in sources {
        for destination in &destinations {
            for kind in ["normal", "build", "dev"] {
                let accepted = domain_allowed(source, destination, kind);
                if source.starts_with("skill-") {
                    assert!(!accepted || destination.starts_with("skill-"));
                }
                if source.starts_with("work-") {
                    assert!(
                        !accepted
                            || destination.starts_with("work-")
                            || *destination == "skill-api"
                    );
                }
                if source.starts_with("tooling-") {
                    assert!(
                        !accepted
                            || destination.starts_with("tooling-")
                            || *destination == "skill-api"
                    );
                }
                if destination.starts_with("skill-")
                    && *destination != "skill-api"
                    && (source.starts_with("work-") || source.starts_with("tooling-"))
                {
                    assert!(!accepted, "{kind} {source} -> {destination}");
                }
            }
        }
    }
    assert!(!domain_allowed("tooling-cli", "skill-api", "normal"));
    assert!(!domain_allowed("work-cli", "tooling-flow", "normal"));
}

#[test]
fn consumers_do_not_name_skill_internal_crates_in_rust_sources() {
    fn inspect(directory: &Path) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                inspect(&path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let content = std::fs::read_to_string(&path).unwrap();
                for suffix in ["model", "operations", "feature", "infrastructure"] {
                    let forbidden = format!("skill_{suffix}::");
                    assert!(
                        !content.contains(&forbidden),
                        "{} uses {forbidden}",
                        path.display()
                    );
                }
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    inspect(&crates.join("work"));
    inspect(&crates.join("tooling"));
}
