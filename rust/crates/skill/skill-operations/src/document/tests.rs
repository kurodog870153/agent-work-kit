use super::*;

const GOOD: &str = "---\nname: 領域指令\ndescription: 適用時使用。\nmetadata:\n  tags:\n    - domain-capability\n---\n\n# 標題\n\n- [Required] 必須遵守。\n- 群組\n  - [Default] 預設遵守。\n\n## References\n\n| Reference | Load condition |\n| --- | --- |\n| `cli-entry.md` | 需要 CLI |\n\n```text\n- 程式碼內不檢查\n```\n";

fn reason(kind: DocumentKind, text: &str) -> Reason {
    parse_document(kind, text).expect_err("rejected").reason
}

#[test]
fn a_valid_instruction_document_parses() {
    let document = parse_document(DocumentKind::Instruction, GOOD).expect("valid");
    assert_eq!(document.routes[0].name, "cli-entry.md");
    assert_eq!(document.frontmatter.expect("frontmatter").name, "領域指令");
}

#[test]
fn decoding_is_strict() {
    assert_eq!(decode(b"\xEF\xBB\xBFabc").unwrap(), "abc");
    assert_eq!(decode(b"abc").unwrap(), "abc");
    assert_eq!(
        decode(b"\xEF\xBB\xBF\xEF\xBB\xBFabc").unwrap_err().reason,
        Reason::MultipleBom
    );
    assert_eq!(decode(b"\xFF\xFE").unwrap_err().reason, Reason::InvalidUtf8);
}

#[test]
fn frontmatter_is_closed_and_validated() {
    let swap = |from: &str, to: &str| GOOD.replacen(from, to, 1);
    assert_eq!(
        reason(DocumentKind::Instruction, "# no frontmatter\n"),
        Reason::MissingFrontmatter
    );
    assert_eq!(
        reason(
            DocumentKind::Instruction,
            &swap("---\n\n# 標題", "\n# 標題")
        ),
        Reason::MissingFrontmatter
    );
    assert_eq!(
        reason(
            DocumentKind::Instruction,
            &swap("metadata:", "extra: 1\nmetadata:")
        ),
        Reason::InvalidFrontmatter
    );
    assert_eq!(
        reason(DocumentKind::Instruction, &swap("領域指令", "' '")),
        Reason::InvalidFrontmatter
    );
    assert_eq!(
        reason(
            DocumentKind::Instruction,
            &swap("domain-capability", "Domain_Capability")
        ),
        Reason::InvalidTags
    );
    assert_eq!(
        reason(
            DocumentKind::Instruction,
            &swap("    - domain-capability", "    - a\n    - a")
        ),
        Reason::InvalidTags
    );
}

#[test]
fn route_tables_are_validated() {
    let swap = |from: &str, to: &str| GOOD.replacen(from, to, 1);
    let kind = DocumentKind::Instruction;
    assert_eq!(
        reason(kind, &swap("`cli-entry.md`", "Cli_Entry.md")),
        Reason::InvalidRouteName
    );
    assert_eq!(reason(kind, &swap("需要 CLI", " ")), Reason::EmptyCondition);
    assert_eq!(
        reason(
            kind,
            &swap(
                "| `cli-entry.md` | 需要 CLI |",
                "| a.md | x |\n| a.md | y |"
            )
        ),
        Reason::DuplicateRoute
    );
    assert_eq!(
        reason(kind, &swap("| Reference |", "| Module |")),
        Reason::InvalidRouteTable
    );
    assert_eq!(
        reason(
            kind,
            &swap(
                "| --- | --- |\n| `cli-entry.md` | 需要 CLI |",
                "| --- | --- |"
            )
        ),
        Reason::InvalidRouteTable
    );
    assert_eq!(
        reason(kind, &swap("## References", "## Modules")),
        Reason::UnexpectedRoutes
    );
    assert_eq!(
        reason(DocumentKind::Reference, GOOD),
        Reason::UnexpectedRoutes
    );
}

#[test]
fn bootstrap_declares_modules_without_frontmatter() {
    let text = "- [Required] 解析入口。\n\n## Modules\n\n| Module | Load condition |\n| --- | --- |\n| source-failure.md | 來源失敗時 |\n";
    let document = parse_document(DocumentKind::Bootstrap, text).expect("valid");
    assert_eq!(document.routes.len(), 1);
    assert!(document.frontmatter.is_none());
    assert_eq!(
        reason(DocumentKind::Instruction, text),
        Reason::MissingFrontmatter
    );
}

#[test]
fn instruction_strength_markers_are_enforced() {
    let wrap = |body: &str| {
        format!("---\nname: n\ndescription: d\nmetadata:\n  tags:\n    - t\n---\n\n{body}\n")
    };
    let kind = DocumentKind::Reference;
    assert_eq!(reason(kind, &wrap("- 沒有標記")), Reason::MissingMarker);
    assert_eq!(
        reason(kind, &wrap("- [Required] [Default] 兩種")),
        Reason::MultipleMarkers
    );
    assert_eq!(
        reason(kind, &wrap("- 說明 [Required]")),
        Reason::MissingMarker
    );
    assert!(parse_document(kind, &wrap("1. [Recommended] 建議。")).is_ok());
}
