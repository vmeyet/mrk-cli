use std::collections::{HashMap, HashSet};

use mermaid_rs_renderer::{DiagramKind, Edge, Graph};

use super::DiagramError;

/// Mermaid blocks above this size are shown as source.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// mermaid-text recurses once per level, and deeper subgraphs never fit a terminal anyway.
const MAX_SUBGRAPH_DEPTH: usize = 16;
/// The limits below keep the slowest diagram measured within them under the times `specs/02-security.md` rule 4 gives.
const MAX_LINKS: usize = 150;
const MAX_MESSAGES: usize = 400;
const MAX_ELEMENTS: usize = 600;
const MAX_STATEMENTS: usize = 100;
/// mermaid-text lays class diagrams out far slower than any other kind: 40 relations took it 2.8 s.
const MAX_RELATIONS: usize = 32;
/// What a document's diagrams may cost together, in milliseconds of layout at the slowest rate measured for what
/// each one counts; a diagram that does not fit what is left shows as source.
pub const MAX_DOCUMENT_COST_MS: usize = 12_000;
/// What any diagram costs at least, its fixed work aside from the layout: at most this many diagrams fit a document.
const MIN_COST_MS: usize = 5;
/// The most diagrams a document lays out.
pub const MAX_DOCUMENT_DIAGRAMS: usize = MAX_DOCUMENT_COST_MS / MIN_COST_MS;
/// What a picture costs on top of its layout: fitting and drawing it.
const PICTURE_COST_MS: usize = 20;

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
    /// Edges, and nodes on none, as the text engine is given them: `a & b --> c & d` is four.
    Statements,
    /// Relations of a class diagram, whose members cost mermaid-text little.
    Relations,
}

impl Load {
    pub fn limit(self) -> usize {
        match self {
            Self::Links => MAX_LINKS,
            Self::Messages => MAX_MESSAGES,
            Self::Elements => MAX_ELEMENTS,
            Self::Statements => MAX_STATEMENTS,
            Self::Relations => MAX_RELATIONS,
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Self::Links => "links",
            Self::Messages => "messages",
            Self::Elements => "elements",
            Self::Statements => "statements",
            Self::Relations => "relations",
        }
    }

    /// The slowest layout measured for that many, in milliseconds: the layered engines slow down per unit as the
    /// diagram grows (145 links between random nodes took 3.1 s, 51 took 0.27 s), the row-by-row ones do not.
    fn cost_ms(self, count: usize) -> usize {
        match self {
            Self::Links => count * count / 7,
            Self::Statements => count * count / 8,
            Self::Relations => count * count * 7 / 4,
            Self::Messages | Self::Elements => count,
        }
    }

    fn within(self, count: usize) -> Result<usize, DiagramError> {
        if count > self.limit() { Err(DiagramError::OverBudget(self)) } else { Ok(self.cost_ms(count)) }
    }
}

/// The diagram as mermaid-rs-renderer parses it, refused before any layout when it is too large to read, its
/// subgraphs would overflow mermaid-text's stack, or it is over its load: layout time grows much faster than the
/// source, which its 64 KiB cap alone does not bound.
pub fn picture_graph(source: &str) -> Result<Graph, DiagramError> {
    let graph = parsed(source)?;
    picture_cost(&graph)?;
    Ok(graph)
}

/// Refuses what mermaid-text would take too long to lay out: counted on mermaid-rs-renderer's graph when it parses the
/// diagram, else on a scan of the source, since mermaid-text reads syntax mermaid-rs-renderer refuses.
pub fn admit_text(source: &str) -> Result<(), DiagramError> {
    admit(source)?;
    text_cost(source, parsed(source).ok().as_ref()).map(|_| ())
}

/// What drawing a diagram costs its document: as a picture when pictures are drawn and its count allows one, else as
/// text; nothing when it is refused before any layout.
pub fn cost(source: &str, pictures: bool) -> usize {
    if admit(source).is_err() {
        return 0;
    }
    let graph = parsed(source).ok();
    let picture = graph.as_ref().filter(|_| pictures).and_then(|graph| picture_cost(graph).ok()).map(|cost| cost + PICTURE_COST_MS);
    picture.or_else(|| text_cost(source, graph.as_ref()).ok()).map_or(0, |cost| cost.max(MIN_COST_MS))
}

/// The diagrams of one document its budget lets through, in reading order, each distinct source counted once since
/// the cache keeps what a repeated one costs: each that fits what is left of the budget.
pub struct Allowance(HashSet<String>);

impl Allowance {
    pub fn new(sources: impl IntoIterator<Item = String>, pictures: bool) -> Self {
        let mut admitted = HashSet::new();
        let mut left = MAX_DOCUMENT_COST_MS;
        for source in sources {
            if admitted.contains(&source) {
                continue;
            }
            let cost = cost(&source, pictures);
            if cost <= left {
                left -= cost;
                admitted.insert(source);
            }
        }
        Self(admitted)
    }

    pub fn admit(&self, source: &str) -> Result<(), DiagramError> {
        if self.0.contains(source) { Ok(()) } else { Err(DiagramError::OverDocumentBudget) }
    }
}

fn parsed(source: &str) -> Result<Graph, DiagramError> {
    admit(source)?;
    let parsed =
        mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|error| DiagramError::Invalid(first_line(&error.to_string())))?;
    Ok(parsed.graph)
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

fn picture_cost(graph: &Graph) -> Result<usize, DiagramError> {
    let (load, count) = picture_load(graph);
    load.within(count)
}

/// The kinds mermaid-text draws in one pass, row after row, cost it only their fixed work: 64 KiB of any of them took
/// it under 30 ms.
fn text_cost(source: &str, graph: Option<&Graph>) -> Result<usize, DiagramError> {
    let load = match graph {
        Some(graph) => graph_text_load(graph),
        None => scanned_text_load(source),
    };
    load.map_or(Ok(MIN_COST_MS), |(load, count)| load.within(count))
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

fn graph_text_load(graph: &Graph) -> Option<(Load, usize)> {
    match graph.kind {
        DiagramKind::Sequence
        | DiagramKind::Pie
        | DiagramKind::Gantt
        | DiagramKind::Timeline
        | DiagramKind::Journey
        | DiagramKind::GitGraph
        | DiagramKind::Mindmap
        | DiagramKind::Kanban => None,
        DiagramKind::Class => Some((Load::Relations, graph.edges.len())),
        _ => {
            let joined: HashSet<&str> = graph.edges.iter().flat_map(|edge| [edge.from.as_str(), edge.to.as_str()]).collect();
            let alone = graph.nodes.keys().filter(|node| !joined.contains(node.as_str())).count();
            Some((Load::Statements, graph.edges.len() + alone + graph.subgraphs.len()))
        }
    }
}

fn scanned_text_load(source: &str) -> Option<(Load, usize)> {
    let header = statements(source).next()?;
    if LINEAR_KINDS.iter().any(|kind| header.starts_with(kind)) {
        None
    } else if header.starts_with("classDiagram") {
        Some((Load::Relations, statements(source).map(|statement| edges_in(statement, &CLASS_ARROWS)).sum()))
    } else {
        Some((Load::Statements, statements(source).skip(1).map(|statement| edges_in(statement, &ARROWS).max(1)).sum()))
    }
}

const LINEAR_KINDS: [&str; 8] = ["sequenceDiagram", "pie", "gantt", "timeline", "journey", "gitGraph", "mindmap", "kanban"];
const ARROWS: [&str; 3] = ["--", "==", "-."];
const CLASS_ARROWS: [&str; 2] = ["--", ".."];
const ARROW_MARK: char = '\u{0}';

/// One edge per pair of nodes either side of each arrow: `a & b --> c & d --> e` is four then two.
fn edges_in(statement: &str, arrows: &[&str]) -> usize {
    let marked = arrows.iter().fold(statement.to_owned(), |marked, arrow| marked.replace(arrow, &ARROW_MARK.to_string()));
    let sides: Vec<usize> = marked.split(ARROW_MARK).map(|side| side.split('&').count()).collect();
    sides.windows(2).map(|pair| pair[0] * pair[1]).sum()
}

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

    fn chain(count: usize, name: &str) -> String {
        edges(count, |index| format!("{name}{index} --> {name}{}", index + 1))
    }

    fn graph(source: &str) -> Graph {
        mermaid_rs_renderer::parse_mermaid(source).unwrap().graph
    }

    fn load_of(source: &str) -> (Load, usize) {
        picture_load(&graph(source))
    }

    fn admitted(allowance: &Allowance, sources: &[String]) -> Vec<bool> {
        sources.iter().map(|source| allowance.admit(source).is_ok()).collect()
    }

    #[test]
    fn a_diagram_costs_what_the_engine_that_draws_it_counts() {
        let flowchart = chain(40, "a");

        assert_eq!(cost(&flowchart, true), Load::Links.cost_ms(41 + 40) + PICTURE_COST_MS);
        assert_eq!(cost(&flowchart, false), Load::Statements.cost_ms(40));
        assert_eq!(cost(&chain(3, "a"), false), MIN_COST_MS);
        assert_eq!(cost("pie\n  \"a\" : 1", false), MIN_COST_MS);
    }

    #[test]
    fn a_diagram_both_engines_refuse_or_drawn_row_after_row_costs_little() {
        assert_eq!(cost(&chain(200, "a"), true), 0);
        assert_eq!(cost(&"%%\n".repeat(MAX_SOURCE_BYTES), true), 0);
        assert_eq!(cost(&format!("sequenceDiagram\n{}", "  a ->> b: hi\n".repeat(1000)), false), MIN_COST_MS);
    }

    #[test]
    fn each_diagram_that_fits_what_is_left_of_the_budget_is_let_through_in_reading_order() {
        let large = |name: &str| chain(MAX_STATEMENTS - 1, name);
        let large_cost = cost(&large("a"), false);
        let fitting = MAX_DOCUMENT_COST_MS / large_cost;
        let sources: Vec<String> = (0..=fitting).map(|index| large(&format!("d{index}_"))).chain([chain(1, "small")]).collect();

        let allowance = Allowance::new(sources.clone(), false);

        let expected: Vec<bool> = (0..=fitting).map(|index| index < fitting).chain([true]).collect();
        assert_eq!(admitted(&allowance, &sources), expected);
        assert_eq!(allowance.admit(&large("z")), Err(DiagramError::OverDocumentBudget));
    }

    #[test]
    fn a_repeated_diagram_costs_its_document_once() {
        let large = chain(MAX_STATEMENTS, "a");
        let repeats = MAX_DOCUMENT_COST_MS / cost(&large, false) + 1;
        let last = chain(MAX_STATEMENTS, "b");

        let allowance = Allowance::new(std::iter::repeat_n(large.clone(), repeats).chain([last.clone()]), false);

        assert_eq!(admitted(&allowance, &[large, last]), [true, true]);
    }

    #[test]
    fn a_document_lets_through_at_most_its_diagram_count() {
        let allowance = Allowance::new((0..2 * MAX_DOCUMENT_DIAGRAMS).map(|index| format!("pie\n  \"n{index}\" : 1")), false);

        assert_eq!(allowance.0.len(), MAX_DOCUMENT_DIAGRAMS);
    }

    #[test]
    fn invalid_source_is_an_error() {
        assert!(matches!(picture_graph("hello"), Err(DiagramError::Invalid(_))));
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
        assert_eq!(load_of("graph TD\n  a & b --> c & d"), (Load::Links, 4 + 4));
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
        assert!(picture_graph(&chain(MAX_LINKS / 2 - 1, "n")).is_ok());
        assert_eq!(picture_graph(&chain(MAX_LINKS / 2 + 1, "n")).unwrap_err(), DiagramError::OverBudget(Load::Links));
    }

    #[test]
    fn parallel_labelled_edges_exhaust_the_budget_quickly() {
        let parallel = edges(30, |_| "a -->|again| b".to_owned());

        assert_eq!(load_of(&parallel), (Load::Links, 2 + 30 + 30 + (0..30).sum::<usize>()));
        assert_eq!(picture_graph(&parallel).unwrap_err(), DiagramError::OverBudget(Load::Links));
    }

    #[test]
    fn the_text_engine_counts_the_parsed_edges_and_lone_nodes() {
        assert_eq!(graph_text_load(&graph("graph TD\n  a --> b --> c\n  d; e")), Some((Load::Statements, 2 + 2)));
        assert_eq!(graph_text_load(&graph("graph TD\n  a & b & c --> d & e & f")), Some((Load::Statements, 9)));
        assert_eq!(graph_text_load(&graph("sequenceDiagram\n  a ->> b: hi")), None);
        assert_eq!(admit_text(&chain(MAX_STATEMENTS, "n")), Ok(()));
        assert_eq!(admit_text(&chain(MAX_STATEMENTS + 1, "n")), Err(DiagramError::OverBudget(Load::Statements)));
    }

    #[test]
    fn the_scan_for_syntax_mermaid_rs_renderer_refuses_counts_every_fanned_out_edge() {
        assert_eq!(scanned_text_load("graph TD\n  a --> b --> c\n  %% a comment\n  d; e"), Some((Load::Statements, 2 + 1 + 1)));
        assert_eq!(scanned_text_load("graph TD\n  a & b & c --> d & e --> f"), Some((Load::Statements, 3 * 2 + 2)));
        assert_eq!(scanned_text_load(&format!("sequenceDiagram\n{}", "  a ->> b: hi\n".repeat(1000))), None);
        assert_eq!(
            scanned_text_load("%% why\nclassDiagram\n  class A {\n    +f() int\n  }\n  A & C --> B\n  B ..> C"),
            Some((Load::Relations, 3))
        );
    }

    #[test]
    fn fanned_out_edges_hidden_on_one_line_are_refused_by_both_engines() {
        let fanned = edges(12, |index| format!("a{index} & b{index} & c{index} --> d{index} & e{index} & f{index}"));

        assert_eq!(picture_graph(&fanned).unwrap_err(), DiagramError::OverBudget(Load::Links));
        assert_eq!(admit_text(&fanned), Err(DiagramError::OverBudget(Load::Statements)));
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
