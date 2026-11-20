//! Pure text rules: strict decoding, frontmatter, route tables and markers.

use std::collections::BTreeSet;

use skill_model::{
    Document, DocumentKind, Frontmatter, InstructionStrength, Reason, Route, RouteKind, SkillError,
};

use crate::path::{is_kebab, is_route_file};

/// Decode bytes as strict UTF-8 with at most one leading BOM.
pub fn decode(bytes: &[u8]) -> Result<&str, SkillError> {
    const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    if body.starts_with(BOM) {
        return Err(SkillError::new(
            Reason::MultipleBom,
            "more than one leading BOM",
        ));
    }
    str::from_utf8(body)
        .map_err(|error| SkillError::new(Reason::InvalidUtf8, format!("not strict UTF-8: {error}")))
}

/// Validate a decoded document and return its declared structure.
pub fn parse_document(kind: DocumentKind, text: &str) -> Result<Document, SkillError> {
    let lines: Vec<&str> = text.lines().collect();
    let (frontmatter, body) = if kind.requires_frontmatter() {
        let (frontmatter, rest) = split_frontmatter(&lines)?;
        (Some(frontmatter), rest)
    } else {
        (None, lines.as_slice())
    };
    let scan = scan_body(body);
    let routes = collect_routes(kind, &scan)?;
    check_markers(&scan.items)?;
    Ok(Document {
        kind,
        frontmatter,
        routes,
    })
}

fn split_frontmatter<'a>(lines: &'a [&'a str]) -> Result<(Frontmatter, &'a [&'a str]), SkillError> {
    if lines.first().copied() != Some("---") {
        return Err(SkillError::new(
            Reason::MissingFrontmatter,
            "frontmatter must start on the first line",
        ));
    }
    let end = lines[1..]
        .iter()
        .position(|line| *line == "---")
        .map(|index| index + 1)
        .ok_or_else(|| SkillError::new(Reason::MissingFrontmatter, "frontmatter is not closed"))?;
    let yaml = lines[1..end].join("\n");
    let frontmatter: Frontmatter = serde_yaml_ng::from_str(&yaml)
        .map_err(|error| SkillError::new(Reason::InvalidFrontmatter, error.to_string()))?;
    check_frontmatter(&frontmatter)?;
    Ok((frontmatter, &lines[end + 1..]))
}

fn check_frontmatter(frontmatter: &Frontmatter) -> Result<(), SkillError> {
    if frontmatter.name.trim().is_empty() || frontmatter.description.trim().is_empty() {
        return Err(SkillError::new(
            Reason::InvalidFrontmatter,
            "name and description must not be empty",
        ));
    }
    let tags = &frontmatter.metadata.tags;
    let unique: BTreeSet<&str> = tags.iter().map(String::as_str).collect();
    if tags.is_empty() || unique.len() != tags.len() || !tags.iter().all(|tag| is_kebab(tag)) {
        return Err(SkillError::new(
            Reason::InvalidTags,
            "tags must be a non-empty, unique list of lower kebab-case strings",
        ));
    }
    Ok(())
}

struct Item {
    indent: usize,
    text: String,
}

#[derive(Default)]
struct Scan {
    /// `(heading, table lines)` for every `## ` heading outside code fences.
    sections: Vec<(String, Vec<String>)>,
    /// List items grouped by heading section; `None` separates sections.
    items: Vec<Option<Item>>,
}

fn scan_body(lines: &[&str]) -> Scan {
    let mut scan = Scan::default();
    let mut fenced = false;
    for line in lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some(title) = line.strip_prefix("## ") {
            scan.sections.push((title.trim().to_owned(), Vec::new()));
            scan.items.push(None);
            continue;
        }
        if line.starts_with('#') {
            scan.items.push(None);
            continue;
        }
        if trimmed.starts_with('|') {
            if let Some((_, table)) = scan.sections.last_mut() {
                table.push(trimmed.to_owned());
            }
            continue;
        }
        if let Some(text) = list_item_text(trimmed) {
            scan.items.push(Some(Item {
                indent: line.len() - trimmed.len(),
                text: text.to_owned(),
            }));
        }
    }
    scan
}

fn list_item_text(trimmed: &str) -> Option<&str> {
    if let Some(rest) = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))
    {
        return Some(rest.trim_start());
    }
    let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        return trimmed[digits..].strip_prefix(". ").map(str::trim_start);
    }
    None
}

fn collect_routes(kind: DocumentKind, scan: &Scan) -> Result<Vec<Route>, SkillError> {
    let mut routes = Vec::new();
    for route_kind in [RouteKind::Module, RouteKind::Reference] {
        let headings: Vec<_> = scan
            .sections
            .iter()
            .filter(|(title, _)| title == route_kind.heading())
            .collect();
        if headings.is_empty() {
            continue;
        }
        if kind.declares() != Some(route_kind) {
            return Err(SkillError::new(
                Reason::UnexpectedRoutes,
                format!("this document may not declare ## {}", route_kind.heading()),
            ));
        }
        let [(_, table)] = headings.as_slice() else {
            return Err(SkillError::new(
                Reason::InvalidRouteTable,
                format!("## {} is declared more than once", route_kind.heading()),
            ));
        };
        routes.extend(parse_table(route_kind, table)?);
    }
    Ok(routes)
}

fn cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_start_matches('|')
        .trim_end_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

fn parse_table(kind: RouteKind, table: &[String]) -> Result<Vec<Route>, SkillError> {
    let invalid = |message: &str| SkillError::new(Reason::InvalidRouteTable, message.to_owned());
    let [header, separator, rows @ ..] = table else {
        return Err(invalid("route table needs a header and separator"));
    };
    if cells(header) != [kind.column(), "Load condition"] {
        return Err(invalid("route table columns do not match the contract"));
    }
    let separator_cells = cells(separator);
    if separator_cells.len() != 2
        || !separator_cells.iter().all(|cell| {
            !cell.is_empty() && cell.chars().all(|character| matches!(character, '-' | ':'))
        })
    {
        return Err(invalid("route table separator row is malformed"));
    }
    if rows.is_empty() {
        return Err(invalid("a declared route table needs at least one row"));
    }
    let mut seen = BTreeSet::new();
    let mut routes = Vec::new();
    for row in rows {
        let [name, condition] = cells(row)
            .as_slice()
            .try_into()
            .map_err(|_| invalid("route rows need exactly a name and a load condition"))?;
        let name = name.trim_matches('`');
        if !is_route_file(name) {
            return Err(SkillError::new(
                Reason::InvalidRouteName,
                format!("route name must be a lower kebab-case .md file: {name}"),
            ));
        }
        if condition.is_empty() {
            return Err(SkillError::new(
                Reason::EmptyCondition,
                format!("load condition is empty: {name}"),
            ));
        }
        if !seen.insert(name.to_owned()) {
            return Err(SkillError::new(
                Reason::DuplicateRoute,
                format!("route declared twice: {name}"),
            ));
        }
        routes.push(Route {
            name: name.to_owned(),
            condition: condition.to_owned(),
        });
    }
    Ok(routes)
}

/// Every leaf list item starts with exactly one strength marker; grouping
/// items that own nested items may omit it, and no item repeats a marker.
fn check_markers(items: &[Option<Item>]) -> Result<(), SkillError> {
    for (index, slot) in items.iter().enumerate() {
        let Some(item) = slot else { continue };
        let found: usize = InstructionStrength::ALL
            .iter()
            .map(|strength| item.text.matches(strength.marker()).count())
            .sum();
        let leading = InstructionStrength::ALL
            .iter()
            .any(|strength| item.text.starts_with(strength.marker()));
        if found > 1 {
            return Err(SkillError::new(
                Reason::MultipleMarkers,
                format!("an instruction uses more than one marker: {}", item.text),
            ));
        }
        if found == 1 && !leading {
            return Err(SkillError::new(
                Reason::MissingMarker,
                format!("a marker must open the instruction: {}", item.text),
            ));
        }
        let is_parent = matches!(
            items.get(index + 1),
            Some(Some(next)) if next.indent > item.indent
        );
        if found == 0 && !is_parent {
            return Err(SkillError::new(
                Reason::MissingMarker,
                format!("instruction has no strength marker: {}", item.text),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
