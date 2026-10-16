use std::collections::{HashMap, HashSet};

use mermaid_rs_renderer::{DiagramKind, Edge, Graph};

use super::DiagramError;
use super::catch::caught;

/// Mermaid blocks above this size are shown as source.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// mermaid-text recurses once per level, and deeper subgraphs never fit a terminal anyway.
const MAX_SUBGRAPH_DEPTH: usize = 16;
/// The limits below keep the slowest diagram measured within them under the times `specs/02-security.md` rule 4 gives.
const MAX_LINKS: usize = 30;
const MAX_MESSAGES: usize = 200;
const MAX_ELEMENTS: usize = 300;

/// What a diagram's layout time grows with, counted before either engine lays it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Load {
    /// Edges, edge labels, subgraphs, and every edge doubling another between the same two nodes, once per earlier one.
    Links,
    /// Messages and notes of a sequence diagram.
    Messages,
    /// Nodes, edges, subgraphs, commits and C4 shapes and relations, for the diagrams laid out in rows or rings.
    Elements,
}

impl Load {
    pub fn limit(self) -> usize {
        match self {
            Self::Links => MAX_LINKS,
            Self::Messages => MAX_MESSAGES,
            Self::Elements => MAX_ELEMENTS,
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Links => "links",
            Self::Messages => "messages",
            Self::Elements => "elements",
        }
    }

    fn of(kind: DiagramKind) -> Self {
        match kind {
            DiagramKind::Flowchart
            | DiagramKind::Class
            | DiagramKind::State
            | DiagramKind::Er
            | DiagramKind::Requirement
            | DiagramKind::Packet
            | DiagramKind::Architecture => Self::Links,
            DiagramKind::Sequence | DiagramKind::ZenUML => Self::Messages,
            _ => Self::Elements,
        }
    }

    fn count(self, graph: &Graph) -> usize {
        match self {
            Self::Links => graph.edges.len() + labels(&graph.edges) + graph.subgraphs.len() + doublings(&graph.edges),
            Self::Messages => graph.edges.len() + graph.sequence_notes.len(),
            Self::Elements => {
                graph.nodes.len()
                    + graph.edges.len()
                    + graph.subgraphs.len()
                    + graph.gitgraph.commits.len()
                    + graph.c4.shapes.len()
                    + graph.c4.rels.len()
            }
        }
    }
}

/// The diagram as mermaid-rs-renderer parses it, refused before any layout when either engine could take too long on
/// it: layout time grows much faster than the source, which its 64 KiB cap alone does not bound.
pub fn parse(source: &str) -> Result<Graph, DiagramError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(DiagramError::Oversized);
    }
    if !has_safe_subgraphs(source) {
        return Err(DiagramError::UnsafeSubgraphs);
    }
    let graph = caught(|| mermaid_rs_renderer::parse_mermaid_strict(source))?
        .map_err(|error| DiagramError::Invalid(first_line(&error.to_string())))?
        .graph;
    let load = Load::of(graph.kind);
    if load.count(&graph) > load.limit() {
        return Err(DiagramError::OverBudget(load));
    }
    Ok(graph)
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
    for statement in source.split(['\n', ';']) {
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

    fn load_of(source: &str) -> usize {
        let graph = mermaid_rs_renderer::parse_mermaid(source).unwrap().graph;
        Load::of(graph.kind).count(&graph)
    }

    #[test]
    fn invalid_source_is_an_error() {
        assert!(matches!(parse("hello"), Err(DiagramError::Invalid(_))));
    }

    #[test]
    fn oversized_source_is_refused_before_parsing() {
        assert_eq!(parse(&"%%\n".repeat(MAX_SOURCE_BYTES)).unwrap_err(), DiagramError::Oversized);
    }

    #[test]
    fn edges_labels_subgraphs_and_doubled_edges_are_links() {
        assert_eq!(load_of("graph TD\n  a --> b\n  b --> c"), 2);
        assert_eq!(load_of("graph TD\n  a -->|yes| b"), 2);
        assert_eq!(load_of("graph TD\n  subgraph s\n  a --> b\n  end"), 2);
        assert_eq!(load_of("graph TD\n  a --> b\n  b --> a\n  a --> b"), 3 + 1 + 2);
    }

    #[test]
    fn each_kind_counts_what_its_layout_grows_with() {
        assert_eq!(Load::of(parse("sequenceDiagram\n  a ->> b: hi\n  note over a: x").unwrap().kind), Load::Messages);
        assert_eq!(load_of("sequenceDiagram\n  a ->> b: hi\n  b ->> a: ho\n  note over a: x"), 3);
        assert_eq!(Load::of(parse("classDiagram\n  A --> B").unwrap().kind), Load::Links);
        assert_eq!(Load::of(parse("pie\n  \"a\" : 1").unwrap().kind), Load::Elements);
        assert_eq!(load_of("gitGraph\n  commit\n  commit"), 2);
    }

    #[test]
    fn a_diagram_at_its_limit_passes_and_one_more_is_refused() {
        let chain = |count| edges(count, |index| format!("n{index} --> n{}", index + 1));

        assert!(parse(&chain(MAX_LINKS)).is_ok());
        assert_eq!(parse(&chain(MAX_LINKS + 1)).unwrap_err(), DiagramError::OverBudget(Load::Links));
    }

    #[test]
    fn parallel_labelled_edges_exhaust_the_budget_quickly() {
        let parallel = edges(8, |_| "a -->|again| b".to_owned());

        assert_eq!(load_of(&parallel), 8 + 8 + (0..8).sum::<usize>());
        assert_eq!(parse(&parallel).unwrap_err(), DiagramError::OverBudget(Load::Links));
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
            assert_eq!(parse(source).unwrap_err(), DiagramError::UnsafeSubgraphs, "{source:?}");
        }
    }

    #[test]
    fn subgraphs_nested_too_deep_are_refused() {
        let nested = |depth: usize| {
            let opening: String = (0..depth).map(|level| format!(" subgraph s{level}\n")).collect();
            format!("graph TD\n{opening} x --> y\n{}", " end\n".repeat(depth))
        };

        assert!(has_safe_subgraphs(&nested(MAX_SUBGRAPH_DEPTH)));
        assert_eq!(parse(&nested(MAX_SUBGRAPH_DEPTH + 1)).unwrap_err(), DiagramError::UnsafeSubgraphs);
    }

    #[test]
    fn distinct_subgraphs_are_safe() {
        assert!(has_safe_subgraphs(
            "graph TD\n subgraph outer\n subgraph inner [Inner box]\n a --> b\n end\n end\n subgraph other\n c\n end"
        ));
    }
}
