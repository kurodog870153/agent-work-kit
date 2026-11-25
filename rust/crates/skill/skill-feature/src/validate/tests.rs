use super::*;
use crate::memory::{Memory, bootstrap, instruction, module, modules, references};

fn valid() -> Memory {
    let mut store = Memory::new();
    store.shared(
        "bootstrap.md",
        &bootstrap(&modules(&[("source-failure.md", "來源失敗時")])),
    );
    store.shared("bootstrap/source-failure.md", &module());
    store.skill("work");
    store.put(
        "work",
        "instructions/instructions.md",
        &instruction("根", &references(&[("overview.md", "需要時")])),
    );
    store.put(
        "work",
        "instructions/references/overview.md",
        &instruction("概覽", ""),
    );
    store.put(
        "work",
        "instructions/task/instructions.md",
        &instruction("任務", ""),
    );
    store
}

fn reasons(store: &Memory) -> Vec<&'static str> {
    let mut found: Vec<_> = validate_all(store)
        .unwrap()
        .failures
        .iter()
        .map(|error| error.reason.code())
        .collect();
    found.sort_unstable();
    found
}

#[test]
fn a_complete_installation_validates() {
    let report = validate_all(&valid()).unwrap();
    assert!(report.is_valid(), "{:?}", report.failures);
    assert_eq!(report.checked.len(), 5);
}

#[test]
fn structural_errors_are_all_reported() {
    let mut store = valid();
    store.put("work", "references/stray.md", "x");
    store.put(
        "work",
        "instructions/references/extra.md",
        &instruction("多餘", ""),
    );
    store.shared("bootstrap/orphan.md", &module());
    store.put("work", "instructions/task/instructions.md", "broken");
    store.dir("work", "instructions/hollow");
    assert_eq!(
        reasons(&store),
        [
            "ancestor_missing",
            "boundary_violation",
            "missing_frontmatter",
            "undeclared_source",
            "undeclared_source"
        ]
    );
}

#[test]
fn declared_but_missing_sources_fail() {
    let mut store = valid();
    store.shared("bootstrap/source-failure.md", "- no marker\n");
    assert_eq!(reasons(&store), ["missing_marker"]);
    let mut store = valid();
    store.shared("bootstrap.md", "no list");
    assert_eq!(reasons(&store), ["undeclared_source"]);
}
