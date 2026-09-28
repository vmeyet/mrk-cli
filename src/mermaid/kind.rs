const KINDS: [(&str, &str); 22] = [
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
    ("zenuml", "sequence diagram"),
];
const UNKNOWN: &str = "diagram";

/// What the diagram is, in words, from its opening keyword: `flowchart`, `sequence diagram`, …
pub fn describe(source: &str) -> &'static str {
    let Some(keyword) = header(source) else { return UNKNOWN };
    KINDS.iter().find(|(prefix, _)| keyword.starts_with(prefix)).map_or(UNKNOWN, |(_, name)| name)
}

fn header(source: &str) -> Option<&str> {
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
}
