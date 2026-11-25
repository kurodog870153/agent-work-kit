use super::*;
use crate::memory::{Memory, bootstrap, instruction, module, modules, references};

fn installed() -> Memory {
    let mut store = Memory::new();
    store.shared(
        "bootstrap.md",
        &bootstrap(&modules(&[("source-failure.md", "來源失敗時")])),
    );
    store.shared("bootstrap/source-failure.md", &module());
    for skill in ["work", "tooling"] {
        store.skill(skill);
        store.put(
            skill,
            "instructions/instructions.md",
            &instruction("根", ""),
        );
    }
    store.put(
        "work",
        "instructions/task/instructions.md",
        &instruction("任務", &references(&[("planning.md", "規劃時")])),
    );
    store.put(
        "work",
        "instructions/task/planning/instructions.md",
        &instruction("規劃", ""),
    );
    store.put(
        "work",
        "instructions/task/references/planning.md",
        &instruction("規劃補充", ""),
    );
    store.put(
        "work",
        "instructions/execute/instructions.md",
        &instruction("執行", ""),
    );
    store
}

fn select(skill: &str, nodes: &[&str]) -> Selection {
    Selection {
        skill: skill.to_owned(),
        nodes: nodes.iter().map(|node| (*node).to_owned()).collect(),
        references: Vec::new(),
    }
}

fn paths(resolution: &Resolution) -> Vec<String> {
    resolution
        .sources
        .iter()
        .map(|loaded| loaded.source.to_string())
        .collect()
}

#[test]
fn a_leaf_selection_loads_the_full_ancestor_chain_only() {
    let store = installed();
    let request = Request {
        selections: vec![select("work", &["task/planning"])],
        modules: Vec::new(),
    };
    let resolution = resolve(&store, &request).expect("valid");
    assert_eq!(
        paths(&resolution),
        [
            "shared:bootstrap.md",
            "skill(work):instructions/instructions.md",
            "skill(work):instructions/task/instructions.md",
            "skill(work):instructions/task/planning/instructions.md",
        ]
    );
    assert_eq!(store.reads("execute/instructions.md"), 0);
}

#[test]
fn sibling_branches_compose_independently_and_deduplicate() {
    let store = installed();
    let request = Request {
        selections: vec![select("work", &["task/planning", "execute"])],
        modules: vec!["source-failure.md".to_owned()],
    };
    let resolution = resolve(&store, &request).expect("valid");
    let root = "skill(work):instructions/instructions.md";
    assert_eq!(
        paths(&resolution)
            .iter()
            .filter(|path| *path == root)
            .count(),
        1
    );
    assert!(
        paths(&resolution).contains(&"skill(work):instructions/execute/instructions.md".to_owned())
    );
    assert!(paths(&resolution).contains(&"shared:bootstrap/source-failure.md".to_owned()));
}

#[test]
fn shared_sources_are_reused_across_skills() {
    let store = installed();
    let request = Request {
        selections: vec![select("work", &["execute"]), select("tooling", &[""])],
        modules: Vec::new(),
    };
    let resolution = resolve(&store, &request).expect("valid");
    let shared = paths(&resolution)
        .iter()
        .filter(|path| path.starts_with("shared:"))
        .count();
    assert_eq!(shared, 1);
    assert_eq!(resolution.sources.len(), 4);
}

#[test]
fn references_load_only_when_declared_on_a_selected_node() {
    let store = installed();
    let with = |node: &str, name: &str| Request {
        selections: vec![Selection {
            references: vec![ReferenceSelection {
                node: node.to_owned(),
                name: name.to_owned(),
            }],
            ..select("work", &["task"])
        }],
        modules: Vec::new(),
    };
    let resolution = resolve(&store, &with("task", "planning.md")).expect("declared");
    assert!(
        paths(&resolution)
            .contains(&"skill(work):instructions/task/references/planning.md".to_owned())
    );
    for bad in [
        with("task", "other.md"),
        with("execute", "planning.md"),
        with("", "planning.md"),
    ] {
        assert_eq!(
            resolve(&store, &bad).unwrap_err().reason,
            Reason::UndeclaredSource
        );
    }
}

#[test]
fn invalid_requests_and_unknown_roots_are_rejected() {
    let store = installed();
    let reason = |request: Request| resolve(&store, &request).unwrap_err().reason;
    assert_eq!(reason(Request::default()), Reason::InvalidRequest);
    assert_eq!(
        reason(Request {
            selections: vec![select("ghost", &[""])],
            modules: Vec::new()
        }),
        Reason::UnknownRoot
    );
    assert_eq!(
        reason(Request {
            selections: vec![select("work", &[""]), select("work", &["task"])],
            modules: Vec::new()
        }),
        Reason::InvalidRequest
    );
    assert_eq!(
        reason(Request {
            selections: vec![select("work", &["../x"])],
            modules: Vec::new()
        }),
        Reason::InvalidName
    );
    assert_eq!(
        reason(Request {
            selections: vec![select("work", &[""])],
            modules: vec!["ghost.md".to_owned()]
        }),
        Reason::UndeclaredSource
    );
}

#[test]
fn failure_scope_follows_the_failed_source() {
    let request = |nodes: &[&str]| Request {
        selections: vec![select("work", nodes)],
        modules: Vec::new(),
    };
    let mut store = installed();
    store.shared("bootstrap.md", "- no marker\n");
    let error = resolve(&store, &request(&[""])).unwrap_err();
    assert_eq!(error.scope, Some(FailureScope::SharedBootstrap));

    let mut store = installed();
    store.put("work", "instructions/instructions.md", "broken");
    assert_eq!(
        resolve(&store, &request(&["task"])).unwrap_err().scope,
        Some(FailureScope::RootOrAncestor)
    );

    let mut store = installed();
    store.put("work", "instructions/task/instructions.md", "broken");
    assert_eq!(
        resolve(&store, &request(&["task/planning"]))
            .unwrap_err()
            .scope,
        Some(FailureScope::RootOrAncestor)
    );
    assert_eq!(
        resolve(&store, &request(&["task"])).unwrap_err().scope,
        Some(FailureScope::Branch)
    );
    assert!(
        resolve(&store, &request(&["execute"])).is_ok(),
        "an unrelated branch stays usable"
    );
}

#[test]
fn a_missing_ancestor_rejects_the_whole_set() {
    let mut store = installed();
    store.put(
        "work",
        "instructions/orphan/leaf/instructions.md",
        &instruction("葉", ""),
    );
    let request = Request {
        selections: vec![select("work", &["orphan/leaf"])],
        modules: Vec::new(),
    };
    let error = resolve(&store, &request).unwrap_err();
    assert_eq!(error.reason, Reason::AncestorMissing);
    assert_eq!(error.scope, Some(FailureScope::RootOrAncestor));
    let mixed = Request {
        selections: vec![select("work", &["orphan/leaf", "execute"])],
        modules: Vec::new(),
    };
    assert!(
        resolve(&store, &mixed).is_err(),
        "no partial set is returned"
    );
}

#[test]
fn switching_reuses_valid_bodies_and_replaces_the_rest() {
    let store = installed();
    let active = resolve(
        &store,
        &Request {
            selections: vec![select("work", &["task"])],
            modules: Vec::new(),
        },
    )
    .unwrap();
    let next = resolve(
        &store,
        &Request {
            selections: vec![select("work", &["execute"])],
            modules: Vec::new(),
        },
    )
    .unwrap();
    let delta = plan_switch(&active, &next);
    let text = |refs: &[SourceRef]| refs.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(
        text(&delta.reused),
        [
            "shared:bootstrap.md",
            "skill(work):instructions/instructions.md"
        ]
    );
    assert_eq!(
        text(&delta.added),
        ["skill(work):instructions/execute/instructions.md"]
    );
    assert_eq!(
        text(&delta.dropped),
        ["skill(work):instructions/task/instructions.md"]
    );
    let same = plan_switch(&active, &active);
    assert!(same.added.is_empty() && same.dropped.is_empty());
}
