const KINDS: [(&str, &str); 29] = [
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
    ("C4Context", "C4 diagram"),
    ("C4Container", "C4 diagram"),
    ("C4Component", "C4 diagram"),
    ("C4Dynamic", "C4 diagram"),
    ("C4Deployment", "C4 diagram"),
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

fn named(source: &str) -> Option<&'static str> {
    let header = header(source)?;
    KINDS.iter().find(|(keyword, _)| opens_on(header, keyword)).map(|(_, name)| *name)
}

/// As mermaid-rs-renderer reads it: case ignored, and the keyword a whole word, so `pieChart-beta` is no pie chart.
fn opens_on(header: &str, keyword: &str) -> bool {
    let Some(rest) = header.get(keyword.len()..) else { return false };
    header[..keyword.len()].eq_ignore_ascii_case(keyword) && !rest.starts_with(|character: char| character.is_ascii_alphanumeric())
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

    #[test]
    fn a_keyword_is_a_whole_word() {
        assert!(!is_known("pieChart-beta\n  \"a\" : 1"));
        assert!(!is_known("C4Landscape\n  Person(a, \"A\")"));
        assert!(is_known("C4Context\n  Person(a, \"A\")"));
        assert!(is_known("graph\n  A --> B"));
        assert_eq!(describe("stateDiagram-v2\n  [*] --> A"), "state diagram");
    }
}
