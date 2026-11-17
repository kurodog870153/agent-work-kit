use super::*;

#[test]
fn kebab_case_is_strict() {
    for good in ["a", "tree-sitter", "a1-b2"] {
        assert!(is_kebab(good), "{good}");
    }
    for bad in ["", "-a", "a-", "a--b", "A", "a_b", "a.b", "任務"] {
        assert!(!is_kebab(bad), "{bad}");
    }
    assert!(is_route_file("cli-entry.md"));
    assert!(!is_route_file("SKILL.md"));
    assert!(!is_route_file("a/b.md"));
    assert!(!is_route_file("cli-entry.txt"));
}

#[test]
fn relative_paths_reject_escapes() {
    assert!(validate_relative("instructions/instructions.md").is_ok());
    for bad in ["", "/abs", "../x", "a/../b", "a//b", "a\\b", "C:/x", "./a"] {
        assert_eq!(
            validate_relative(bad).unwrap_err().reason,
            Reason::InvalidPath,
            "{bad}"
        );
    }
}

#[test]
fn hierarchy_layout_is_derived_from_the_node_path() {
    assert_eq!(ancestor_chain(""), [""]);
    assert_eq!(
        ancestor_chain("task/planning"),
        ["", "task", "task/planning"]
    );
    assert_eq!(node_file(""), "instructions/instructions.md");
    assert_eq!(node_file("task"), "instructions/task/instructions.md");
    assert_eq!(
        route_path(RouteKind::Reference, "task", "a.md"),
        "instructions/task/references/a.md"
    );
    assert_eq!(route_path(RouteKind::Module, "", "a.md"), "bootstrap/a.md");
    assert!(validate_node("task/Plan").is_err());
}
