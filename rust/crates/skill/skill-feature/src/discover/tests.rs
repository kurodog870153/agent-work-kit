use super::*;
use crate::memory::{Memory, instruction};

#[test]
fn only_direct_children_are_read_and_bad_siblings_are_reported() {
    let mut store = Memory::new();
    store.skill("work");
    store.put(
        "work",
        "instructions/instructions.md",
        &instruction("根", ""),
    );
    store.put(
        "work",
        "instructions/task/instructions.md",
        &instruction("任務", ""),
    );
    store.put("work", "instructions/task/deep/instructions.md", "broken");
    store.put(
        "work",
        "instructions/execute/instructions.md",
        "no frontmatter",
    );
    store.dir("work", "instructions/hollow");
    store.dir("work", "instructions/Bad_Name");
    store.put("work", "instructions/references/a.md", "x");
    let found = discover(&store, "work", "").expect("root is valid");
    let nodes: Vec<_> = found
        .children
        .iter()
        .map(|child| child.node.as_str())
        .collect();
    assert_eq!(nodes, ["task"]);
    let mut reasons: Vec<_> = found
        .failures
        .iter()
        .map(|error| error.reason.code())
        .collect();
    reasons.sort_unstable();
    assert_eq!(
        reasons,
        ["ancestor_missing", "invalid_name", "missing_frontmatter"]
    );
    assert!(
        found
            .failures
            .iter()
            .all(|error| error.scope == Some(FailureScope::Branch))
    );
    assert_eq!(store.reads("instructions/task/deep/instructions.md"), 0);
}

#[test]
fn an_invalid_root_rejects_discovery_with_root_scope() {
    let mut store = Memory::new();
    store.skill("work");
    let error = discover(&store, "work", "").unwrap_err();
    assert_eq!(error.scope, Some(FailureScope::RootOrAncestor));
    assert_eq!(
        discover(&store, "work", "../x").unwrap_err().reason,
        Reason::InvalidName
    );
}
