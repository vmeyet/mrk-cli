const KINDS: [(&str, &str); 24] = [
    ("graph", "flowchart"),
    ("flowchart", "flowchart"),
    ("sequenceDiagram", "sequence diagram"),
    ("classDiagram", "class diagram"),
    ("stateDiagram", "state diagram"),
    ("erDiagram", "entity relationship diagram"),
    ("journey", "user journey"),
    ("gantt", "gantt chart"),
    ("pie", "pie chart"),
    ("quadrantChart", "quadrant chart"),
    ("requirementDiagram", "requirement diagram"),
    ("gitGraph", "git graph"),
    ("C4", "C4 diagram"),
    ("mindmap", "mind map"),
    ("timeline", "timeline"),
    ("sankey", "sankey diagram"),
    ("xychart", "xy chart"),
    ("block", "block diagram"),
    ("packet", "packet diagram"),
    ("kanban", "kanban board"),
    ("architecture", "architecture diagram"),
    ("radar", "radar chart"),
    ("treemap", "treemap"),
    ("zenuml", "sequence diagram"),
];
const UNKNOWN: &str = "diagram";

/// What the diagram is, in words, from its opening keyword: `flowchart`, `sequence diagram`, …
pub fn describe(source: &str) -> &'static str {
    named(source).unwrap_or(UNKNOWN)
}

/// Whether the opening keyword names a kind mrk draws: mermaid-rs-renderer reads a keyword it does not know, such as
/// `venn-beta`, as a flowchart.
pub(super) fn is_known(source: &str) -> bool {
    named(source).is_some()
}

/// Matched ignoring case, as both engines read the keyword.
fn named(source: &str) -> Option<&'static str> {
    let keyword = header(source)?;
    KINDS
        .iter()
        .find(|(prefix, _)| keyword.get(..prefix.len()).is_some_and(|start| start.eq_ignore_ascii_case(prefix)))
        .map(|(_, name)| *name)
}

/// The line that opens the diagram, after any front matter and comment.
pub(super) fn header(source: &str) -> Option<&str> {
    let body = skip_front_matter(source);
    body.lines().map(str::trim).find(|line| !line.is_empty() && !line.starts_with("%%"))
}

fn skip_front_matter(source: &str) -> &str {
    let trimmed = source.trim_start();
    let Some(rest) = trimmed.strip_prefix("---") else { return source };
    rest.split_once("\n---").map_or(source, |(_, body)| body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_keyword_names_the_kind() {
        assert_eq!(describe("graph TD\n  A --> B"), "flowchart");
        assert_eq!(describe("  flowchart LR; A-->B"), "flowchart");
        assert_eq!(describe("sequenceDiagram\n  A->>B: hi"), "sequence diagram");
        assert_eq!(describe("stateDiagram-v2\n  [*] --> A"), "state diagram");
    }

    #[test]
    fn front_matter_and_directives_are_skipped() {
        assert_eq!(describe("---\ntitle: x\n---\n%%{init: {}}%%\n\npie\n  \"a\": 1"), "pie chart");
    }

    #[test]
    fn unknown_or_empty_source_is_a_diagram() {
        assert_eq!(describe(""), "diagram");
        assert_eq!(describe("hello world"), "diagram");
        assert_eq!(describe("---\nunterminated"), "diagram");
    }

    #[test]
    fn the_keyword_is_read_ignoring_case() {
        assert_eq!(describe("Graph LR\n  A --> B"), "flowchart");
        assert!(is_known("GITGRAPH\n  commit"));
    }

    #[test]
    fn a_keyword_naming_no_kind_is_unknown() {
        assert!(!is_known("venn-beta\n  set A"));
        assert!(!is_known("roadmap-beta\n  step one"));
        assert!(is_known("radar-beta\n  axis a, b"));
    }
}
