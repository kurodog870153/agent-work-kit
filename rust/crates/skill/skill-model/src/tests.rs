use super::*;

#[test]
fn markers_and_route_kinds_are_fixed() {
    let markers: Vec<_> = InstructionStrength::ALL
        .map(InstructionStrength::marker)
        .into();
    assert_eq!(markers, ["[Required]", "[Default]", "[Recommended]"]);
    assert_eq!(DocumentKind::Bootstrap.declares(), Some(RouteKind::Module));
    assert_eq!(
        DocumentKind::Instruction.declares(),
        Some(RouteKind::Reference)
    );
    assert_eq!(DocumentKind::Reference.declares(), None);
    assert!(!DocumentKind::Module.requires_frontmatter());
}

#[test]
fn errors_keep_the_first_source_and_display_the_reason() {
    let error = SkillError::new(Reason::InvalidUtf8, "bad")
        .with_source("a")
        .with_source("b");
    assert_eq!(error.source.as_deref(), Some("a"));
    assert_eq!(error.to_string(), "invalid_utf8: bad");
    assert_eq!(
        SourceRef::skill("work", "instructions/instructions.md").to_string(),
        "skill(work):instructions/instructions.md"
    );
}
