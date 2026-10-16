use std::collections::{HashMap, HashSet};

use mermaid_rs_renderer::{DiagramKind, Edge, Graph};

use super::DiagramError;

/// Mermaid blocks above this size are shown as source.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// mermaid-text recurses once per level, and deeper subgraphs never fit a terminal anyway.
const MAX_SUBGRAPH_DEPTH: usize = 16;
/// The limits below keep the slowest diagram measured within them under the times `specs/02-security.md` rule 4 gives.
const MAX_LINKS: usize = 300;
const MAX_MESSAGES: usize = 400;
const MAX_ELEMENTS: usize = 600;
const MAX_STATEMENTS: usize = 160;
/// mermaid-text lays class diagrams out far slower than any other kind: 40 relations took it 2.8 s.
const MAX_CLASS_STATEMENTS: usize = 32;

/// What a diagram's layout time grows with, counted before an engine lays it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    /// Nodes, edges, edge labels, subgraphs, and every edge doubling another between the same two nodes, once per
    /// earlier one: what the layered layout routes around.
    Links,
    /// Participants, messages and notes of a sequence diagram.
    Messages,
    /// Nodes, edges, subgraphs, commits and C4 shapes and relations, for the diagrams laid out in rows or rings.
    Elements,
    /// Statements, one more per arrow chained after the first, as the text engine is given them.
    Statements,
    /// The same, for a class diagram.
    ClassStatements,
}

impl Load {
    pub fn limit(self) -> usize {
        match self {
            Self::Links => MAX_LINKS,
            Self::Messages => MAX_MESSAGES,
            Self::Elements => MAX_ELEMENTS,
            Self::Statements => MAX_STATEMENTS,
            Self::ClassStatements => MAX_CLASS_STATEMENTS,
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Links => "links",
            Self::Messages => "messages",
            Self::Elements => "elements",
            Self::Statements | Self::ClassStatements => "statements",
        }
    }

    fn within(self, count: usize) -> Result<(), DiagramError> {
        if count > self.limit() { Err(DiagramError::OverBudget(self)) } else { Ok(()) }
    }
}

/// Refuses, for both engines, a block too large to read or whose subgraphs would overflow mermaid-text's stack.
pub fn admit(source: &str) -> Result<(), DiagramError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(DiagramError::Oversized);
    }
    if !has_safe_subgraphs(source) {
        return Err(DiagramError::UnsafeSubgraphs);
    }
    Ok(())
}

/// The diagram as mermaid-rs-renderer parses it, refused before any layout when it is over its load: layout time
/// grows much faster than the source, which its 64 KiB cap alone does not bound.
pub fn parse(source: &str) -> Result<Graph, DiagramError> {
    let graph =
        mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|error| DiagramError::Invalid(first_line(&error.to_string())))?.graph;
    let (load, count) = picture_load(&graph);
    load.within(count)?;
    Ok(graph)
}

/// Refuses a diagram with more statements than mermaid-text lays out quickly, read off the source without parsing it:
/// mermaid-text accepts syntax mermaid-rs-renderer's parser refuses.
pub fn admit_text(source: &str) -> Result<(), DiagramError> {
    let (load, count) = text_load(source);
    load.within(count)
}

fn picture_load(graph: &Graph) -> (Load, usize) {
    let shapes = graph.nodes.len() + graph.edges.len() + graph.subgraphs.len();
    match graph.kind {
        DiagramKind::Flowchart
        | DiagramKind::Class
        | DiagramKind::State
        | DiagramKind::Er
        | DiagramKind::Requirement
        | DiagramKind::Architecture => (Load::Links, shapes + labels(&graph.edges) + doublings(&graph.edges)),
        DiagramKind::Sequence | DiagramKind::ZenUML => (Load::Messages, graph.nodes.len() + graph.edges.len() + graph.sequence_notes.len()),
        _ => (Load::Elements, shapes + graph.gitgraph.commits.len() + graph.c4.shapes.len() + graph.c4.rels.len()),
    }
}

fn text_load(source: &str) -> (Load, usize) {
    let count = statements(source).map(|statement| ARROWS.iter().map(|arrow| statement.matches(arrow).count()).sum::<usize>().max(1)).sum();
    let is_class = statements(source).next().is_some_and(|header| header.starts_with("classDiagram"));
    (if is_class { Load::ClassStatements } else { Load::Statements }, count)
}

const ARROWS: [&str; 3] = ["--", "==", "-."];

fn statements(source: &str) -> impl Iterator<Item = &str> {
    source.split(['\n', ';']).map(str::trim).filter(|statement| !statement.is_empty() && !statement.starts_with("%%"))
}

fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or("invalid diagram").to_owned()
}

fn labels(edges: &[Edge]) -> usize {
    edges.iter().flat_map(|edge| [&edge.label, &edge.start_label, &edge.end_label]).filter(|label| label.is_some()).count()
}

/// Edges between the same two nodes route and label around each other, so each costs one more per edge before it.
fn doublings(edges: &[Edge]) -> usize {
    let mut seen: HashMap<(&str, &str), usize> = HashMap::new();
    edges
        .iter()
        .map(|edge| {
            let pair = if edge.from <= edge.to { (edge.from.as_str(), edge.to.as_str()) } else { (edge.to.as_str(), edge.from.as_str()) };
            let earlier = seen.entry(pair).or_default();
            *earlier += 1;
            *earlier - 1
        })
        .sum()
}

/// mermaid-text recurses without end on a subgraph that reuses an id, and a stack overflow aborts past `catch_unwind`.
/// Statements are split the way mermaid-text splits them: on newlines and semicolons.
fn has_safe_subgraphs(source: &str) -> bool {
    let mut ids = HashSet::new();
    let mut depth = 0_usize;
    for statement in statements(source) {
        match statement.split_whitespace().next() {
            Some("subgraph") => {
                depth += 1;
                if depth > MAX_SUBGRAPH_DEPTH || !ids.insert(subgraph_id(statement)) {
                    return false;
                }
            }
            Some("end") => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    true
}

/// `subgraph id`, `subgraph id [label]`, or `subgraph [label]` where the label stands for the id.
fn subgraph_id(statement: &str) -> &str {
    let header = statement.trim().trim_start_matches("subgraph").trim();
    match header.split_once('[') {
        Some((id, _)) if !id.trim().is_empty() => id.trim(),
        Some((_, label)) => label.split_once(']').map_or(label, |(inside, _)| inside).trim(),
        None => header,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn edges(count: usize, edge: impl Fn(usize) -> String) -> String {
        format!("graph TD\n{}", (0..count).map(|index| format!("  {}\n", edge(index))).collect::<String>())
    }

    fn load_of(source: &str) -> (Load, usize) {
        picture_load(&mermaid_rs_renderer::parse_mermaid(source).unwrap().graph)
    }

    #[test]
    fn invalid_source_is_an_error() {
        assert!(matches!(parse("hello"), Err(DiagramError::Invalid(_))));
    }

    #[test]
    fn oversized_source_is_refused_before_parsing() {
        assert_eq!(admit(&"%%\n".repeat(MAX_SOURCE_BYTES)), Err(DiagramError::Oversized));
    }

    #[test]
    fn nodes_edges_labels_subgraphs_and_doubled_edges_are_links() {
        assert_eq!(load_of("graph TD\n  a --> b\n  b --> c"), (Load::Links, 3 + 2));
        assert_eq!(load_of("graph TD\n  a -->|yes| b"), (Load::Links, 2 + 1 + 1));
        assert_eq!(load_of("graph TD\n  subgraph s\n  a --> b\n  end"), (Load::Links, 2 + 1 + 1));
        assert_eq!(load_of("graph TD\n  a --> b\n  b --> a\n  a --> b"), (Load::Links, 2 + 3 + (1 + 2)));
        assert_eq!(load_of("classDiagram\n  A --> B").0, Load::Links);
    }

    #[test]
    fn each_kind_counts_what_its_layout_grows_with() {
        assert_eq!(load_of("sequenceDiagram\n  a ->> b: hi\n  b ->> a: ho\n  note over a: x"), (Load::Messages, 2 + 2 + 1));
        assert_eq!(load_of("pie\n  \"a\" : 1").0, Load::Elements);
        assert_eq!(load_of("packet-beta\n  0-15: \"Source\"").0, Load::Elements);
        assert_eq!(load_of("gitGraph\n  commit\n  commit"), (Load::Elements, 2));
    }

    #[test]
    fn a_diagram_at_its_limit_passes_and_one_more_is_refused() {
        let chain = |links: usize| edges(links / 2, |index| format!("n{index} --> n{}", index + 1));

        assert!(parse(&chain(MAX_LINKS - 1)).is_ok());
        assert_eq!(parse(&chain(MAX_LINKS + 2)).unwrap_err(), DiagramError::OverBudget(Load::Links));
    }

    #[test]
    fn parallel_labelled_edges_exhaust_the_budget_quickly() {
        let parallel = edges(30, |_| "a -->|again| b".to_owned());

        assert_eq!(load_of(&parallel), (Load::Links, 2 + 30 + 30 + (0..30).sum::<usize>()));
        assert_eq!(parse(&parallel).unwrap_err(), DiagramError::OverBudget(Load::Links));
    }

    #[test]
    fn the_text_engine_counts_statements_and_chained_arrows() {
        assert_eq!(text_load("graph TD\n  a --> b --> c\n  %% a comment\n  d; e"), (Load::Statements, 1 + 2 + 1 + 1));
        assert_eq!(text_load("sequenceDiagram\n  a ->> b: hi"), (Load::Statements, 2));
        assert_eq!(text_load("%% why\nclassDiagram\n  A --> B"), (Load::ClassStatements, 2));
        assert_eq!(admit_text(&edges(MAX_STATEMENTS - 1, |index| format!("n{index} --> n{}", index + 1))), Ok(()));
        assert_eq!(admit_text(&edges(MAX_STATEMENTS, |index| format!("n{index}"))), Err(DiagramError::OverBudget(Load::Statements)));
    }

    #[test]
    fn syntax_only_mermaid_text_reads_passes_the_text_gate() {
        assert_eq!(admit_text("classDiagram\n  class Shape {\n    <<interface>>\n    +area() double\n  }"), Ok(()));
    }

    #[test]
    fn a_reused_subgraph_id_is_refused_before_parsing() {
        let sources = [
            "graph TD\n subgraph a\n subgraph a\n end\n end",
            "graph TD; subgraph a; subgraph a; end; end",
            "graph TD\n subgraph a [One]\n end\n subgraph b\n subgraph a[Two]\n end\n end",
            "graph TD\n subgraph [Same]\n subgraph [Same]\n end\n end",
            "graph TD\n subgraph\n subgraph\n end\n end",
        ];

        for source in sources {
            assert_eq!(admit(source), Err(DiagramError::UnsafeSubgraphs), "{source:?}");
        }
    }

    #[test]
    fn subgraphs_nested_too_deep_are_refused() {
        let nested = |depth: usize| {
            let opening: String = (0..depth).map(|level| format!(" subgraph s{level}\n")).collect();
            format!("graph TD\n{opening} x --> y\n{}", " end\n".repeat(depth))
        };

        assert_eq!(admit(&nested(MAX_SUBGRAPH_DEPTH)), Ok(()));
        assert_eq!(admit(&nested(MAX_SUBGRAPH_DEPTH + 1)), Err(DiagramError::UnsafeSubgraphs));
    }

    #[test]
    fn distinct_subgraphs_are_safe() {
        assert!(has_safe_subgraphs(
            "graph TD\n subgraph outer\n subgraph inner [Inner box]\n a --> b\n end\n end\n subgraph other\n c\n end"
        ));
    }
}
