mod budget;
mod cache;
mod catch;
mod frame;
mod kind;
mod source;
mod svg;
mod text;
mod worker;

use std::fmt;
use std::sync::Arc;
use std::time::Instant;

pub(crate) use self::budget::Allowance;
use self::budget::Load;
use self::cache::Recent;
use self::catch::caught;
pub use self::catch::is_catching;
use crate::document::{Block, CellSize, Line, Picture, Settings};
use crate::raster::{RasterError, Svg};

static DRAWINGS: Recent<Result<Arc<Svg>, DiagramError>> = Recent::new();
/// The text a diagram was last drawn as, with the width it was drawn for.
static TEXTS: Recent<(usize, Text)> = Recent::new();

type Text = Result<Arc<Vec<Line>>, DiagramError>;

/// Why a diagram is not drawn; the source fallback shows it in its note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiagramError {
    Oversized,
    UnsafeSubgraphs,
    OverBudget(Load),
    OverDocumentBudget,
    TimedOut,
    Invalid(String),
    Crashed,
    Empty,
    TooWide(usize),
    TooLarge,
    UnreadableSvg(String),
    PngEncoding(String),
}

impl fmt::Display for DiagramError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oversized => write!(formatter, "diagram over {} KiB", budget::MAX_SOURCE_BYTES / 1024),
            Self::UnsafeSubgraphs => write!(formatter, "subgraph id reused or nested too deep"),
            Self::OverBudget(load) => write!(formatter, "over {} {}, too many to lay out", load.limit(), load.unit()),
            Self::OverDocumentBudget => {
                write!(formatter, "over {} statements across the document's diagrams, too many to lay out", budget::MAX_DOCUMENT_STATEMENTS)
            }
            Self::TimedOut => write!(formatter, "too slow to lay out"),
            Self::Invalid(message) => write!(formatter, "{message}"),
            Self::Crashed => write!(formatter, "the diagram renderer crashed"),
            Self::Empty => write!(formatter, "nothing to draw"),
            Self::TooWide(width) => write!(formatter, "the diagram is wider than {width} columns"),
            Self::TooLarge => write!(formatter, "diagram too large to draw"),
            Self::UnreadableSvg(message) => write!(formatter, "unreadable SVG ({message})"),
            Self::PngEncoding(message) => write!(formatter, "PNG encoding failed ({message})"),
        }
    }
}

impl From<RasterError> for DiagramError {
    fn from(error: RasterError) -> Self {
        match error {
            RasterError::UnreadableSvg(message) => Self::UnreadableSvg(message),
            RasterError::NoCanvas => Self::Empty,
            RasterError::PngEncoding(message) => Self::PngEncoding(message),
        }
    }
}

/// A ```mermaid block on its own: a picture when `settings.cell` is set and the diagram renders, text otherwise.
pub fn render(source: &str, settings: &Settings) -> Block {
    drawn(source, settings).unwrap_or_else(|error| as_source(source, &error, settings))
}

/// A ```mermaid block of a document, shown as source when the document's budget does not let it through.
pub(crate) fn render_in(source: &str, settings: &Settings, allowance: &Allowance) -> Block {
    allowance.admit(source).and_then(|()| drawn(source, settings)).unwrap_or_else(|error| as_source(source, &error, settings))
}

fn drawn(source: &str, settings: &Settings) -> Result<Block, DiagramError> {
    budget::admit(source)?;
    let deadline = settings.diagram_deadline.map(|deadline| Instant::now() + deadline);
    match settings.cell.and_then(|cell| picture(source, cell, settings, deadline).ok()) {
        Some(picture) => Ok(Block::Picture(picture)),
        None => as_text(source, settings, deadline),
    }
}

/// Parsed, laid out and turned into a tree once per source and palette, a timeout included: only the fit and the
/// raster depend on the width.
fn picture(source: &str, cell: CellSize, settings: &Settings, deadline: Option<Instant>) -> Result<Picture, DiagramError> {
    let drawing = DRAWINGS.get_or(source, &settings.theme.palette, || drawing(source, settings, deadline))?;
    let (width, height) = drawing.size();
    let frame = frame::fit(width, height, svg::FONT_SIZE_PX, cell, settings.width).ok_or(DiagramError::TooLarge)?;
    let png = caught(|| drawing.draw(frame.scale, frame.width_px, frame.height_px))??;
    let alt = kind::describe(source).to_owned();
    Ok(Picture { png, cols: frame.cols, rows: frame.rows, alt, indent: Line::blank(), concealed_text: None })
}

/// Only the layout runs against the deadline, so the fonts the tree loads on first use are never left to a thread
/// given up on.
fn drawing(source: &str, settings: &Settings, deadline: Option<Instant>) -> Result<Arc<Svg>, DiagramError> {
    let (source, palette) = (source.to_owned(), settings.theme.palette);
    let svg = worker::run(deadline, move || Ok(svg::render(&budget::parse(&source)?, &palette)))?;
    Ok(Arc::new(caught(|| Svg::parse(&svg))??))
}

/// mermaid-text lays out for the width, so it runs again after a resize, unless it once ran out of time.
fn as_text(source: &str, settings: &Settings, deadline: Option<Instant>) -> Result<Block, DiagramError> {
    budget::admit_text(source)?;
    let (palette, width) = (settings.theme.palette, settings.width);
    let known = TEXTS.find(source, &palette).filter(|(known_width, text)| *known_width == width || *text == Err(DiagramError::TimedOut));
    let (_, text) = known.unwrap_or_else(|| {
        let owned = source.to_owned();
        let drawn = (width, worker::run(deadline, move || text::render(&owned, width, &palette)).map(Arc::new));
        TEXTS.keep(source, &palette, drawn.clone());
        drawn
    });
    text.map(|lines| Block::Lines(lines.to_vec()))
}

fn as_source(source: &str, error: &DiagramError, settings: &Settings) -> Block {
    Block::Lines(source::render(source, &error.to_string(), settings.width, &settings.theme.palette))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::time::Duration;

    use super::*;
    use crate::document::Line;
    use crate::document::test_settings;

    const FLOWCHART: &str = "graph TD\n  A[Start] --> B{Ready?}\n  B -->|yes| C[Render]\n  B -->|no| D[Wait]\n  D --> B\n";
    const SEQUENCE: &str = "sequenceDiagram\n  Alice->>Bob: Hello\n  Bob-->>Alice: Hi\n";
    const CELL: CellSize = CellSize { width_px: 16, height_px: 36 };

    fn with_cell() -> Settings {
        Settings { cell: Some(CELL), ..test_settings() }
    }

    fn lines(block: Block) -> Vec<Line> {
        match block {
            Block::Lines(lines) => lines,
            other => panic!("expected lines, got {other:?}"),
        }
    }

    fn picture_of(source: &str) -> Picture {
        match render(source, &with_cell()) {
            Block::Picture(picture) => picture,
            other => panic!("expected a picture, got {other:?}"),
        }
    }

    fn assert_fills_its_cells(picture: &Picture) {
        let drawn = crate::raster::inspect(&picture.png).unwrap();
        assert_eq!(drawn.width, u32::from(picture.cols) * u32::from(CELL.width_px));
        assert_eq!(drawn.height, u32::from(picture.rows) * u32::from(CELL.height_px));

        assert!(usize::from(picture.cols) <= with_cell().width);
        assert!(drawn.has_ink);
    }

    #[test]
    fn flowchart_becomes_a_picture() {
        let picture = picture_of(FLOWCHART);

        assert_eq!(picture.alt, "flowchart");
        assert!(picture.rows >= 5, "rows {}", picture.rows);
        assert_fills_its_cells(&picture);
    }

    #[test]
    fn sequence_diagram_becomes_a_picture() {
        let picture = picture_of(SEQUENCE);

        assert_eq!(picture.alt, "sequence diagram");
        assert!(picture.cols >= 10, "cols {}", picture.cols);
        assert_fills_its_cells(&picture);
    }

    #[test]
    fn without_a_cell_size_the_diagram_is_text() {
        let lines = lines(render(FLOWCHART, &test_settings()));

        assert!(lines.iter().any(|line| line.plain().contains("Render")));
        assert!(lines.iter().all(|line| line.width() <= 80));
    }

    #[test]
    fn oversized_source_is_shown_as_source() {
        let source = format!("graph TD\n{}", "  A --> B\n".repeat(7000));

        let lines = lines(render(&source, &with_cell()));

        assert_eq!(lines[0].plain(), "mermaid: diagram over 64 KiB");
        assert_eq!(lines[1].plain(), "graph TD");
    }

    #[test]
    fn a_subgraph_inside_itself_is_shown_as_source_by_both_engines() {
        for settings in [test_settings(), with_cell()] {
            let lines = lines(render("graph TD\n subgraph a\n subgraph a\n end\n end", &settings));

            assert_eq!(lines[0].plain(), "mermaid: subgraph id reused or nested too deep");
            assert_eq!(lines[1].plain(), "graph TD");
        }
    }

    /// Edges between pseudo-random nodes, the shape that took the engines longest to lay out.
    fn tangled(nodes: usize, edges: usize) -> String {
        let edge = |index: usize| format!("  n{} --> n{}\n", index * 7919 % nodes, index * 104_729 % nodes);
        format!("graph TD\n{}", (0..edges).map(edge).collect::<String>())
    }

    #[test]
    fn diagrams_too_costly_to_lay_out_are_refused_before_either_engine() {
        let costly = [
            (tangled(100, 500), "mermaid: over 100 statements, too many to lay out"),
            (tangled(1000, 3500), "mermaid: over 100 statements, too many to lay out"),
            (format!("classDiagram\n{}", "  A --> B\n".repeat(40)), "mermaid: over 32 relations, too many to lay out"),
        ];

        for (source, note) in &costly {
            assert!(source.len() <= budget::MAX_SOURCE_BYTES, "{note}: {} bytes", source.len());
            for settings in [test_settings(), with_cell()] {
                assert_eq!(lines(render(source, &settings))[0].plain(), *note);
            }
        }
    }

    #[test]
    fn the_deepest_subgraphs_the_budget_admits_lay_out_on_a_default_thread_stack() {
        let opening: String = (0..16).map(|level| format!(" subgraph s{level}\n")).collect();
        let source = format!("graph TD\n{opening} x --> y\n{}", " end\n".repeat(16));
        assert_eq!(budget::admit(&source), Ok(()));

        let drawn = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || [test_settings(), with_cell()].map(|settings| render(&source, &settings)))
            .unwrap()
            .join()
            .unwrap();

        assert!(matches!(&drawn, [Block::Lines(text), Block::Picture(_)] if !text[0].plain().starts_with("mermaid:")), "{drawn:?}");
    }

    #[test]
    fn a_long_sequence_over_the_picture_budget_still_draws_as_text() {
        let source = format!("sequenceDiagram\n{}", "  a ->> b: hi\n".repeat(1000));

        let lines = lines(render(&source, &with_cell()));

        assert!(lines.iter().any(|line| line.plain().contains("hi")));
        assert!(!lines[0].plain().starts_with("mermaid:"));
    }

    #[test]
    fn a_diagram_out_of_time_shows_as_source_and_stays_so_on_a_resize() {
        let source = tangled(20, 40);

        let out_of_time = lines(render(&source, &Settings { diagram_deadline: Some(Duration::ZERO), ..with_cell() }));
        let resized = lines(render(&source, &Settings { width: 60, ..with_cell() }));

        assert_eq!(out_of_time[0].plain(), "mermaid: too slow to lay out");
        assert_eq!(resized[0].plain(), "mermaid: too slow to lay out");
    }

    #[test]
    fn a_diagram_past_the_document_budget_shows_as_source_with_its_note() {
        let lines = lines(render_in(FLOWCHART, &with_cell(), &Allowance::new([])));

        assert_eq!(lines[0].plain(), "mermaid: over 400 statements across the document's diagrams, too many to lay out");
        assert_eq!(lines[1].plain(), "graph TD");
    }

    #[test]
    fn a_document_lays_out_its_diagrams_until_their_statements_reach_the_budget() {
        let chain = |diagram: usize| (0..99).map(|node| format!("  d{diagram}n{node} --> d{diagram}n{}\n", node + 1)).collect::<String>();
        let document: String = (0..5).map(|diagram| format!("```mermaid\ngraph TD\n{}```\n\n", chain(diagram))).collect();

        let plain = crate::document::plain(&crate::markdown::render(&document, &test_settings()));

        assert_eq!(plain.matches("mermaid: over 400 statements across the document's diagrams").count(), 1);
        assert!(plain.contains("d3n99") && plain.trim_end().ends_with("d4n98 --> d4n99"), "{plain}");
    }

    #[test]
    fn syntax_only_mermaid_text_reads_still_draws_as_text() {
        let sources = [
            "classDiagram\n  class Shape {\n    <<interface>>\n    +area() float\n  }\n  Shape <|-- Circle",
            "sequenceDiagram\n  box Purple Front\n    participant A\n  end\n  A->>B: hi\n  B-->>A: ok",
        ];

        for source in sources {
            let lines = lines(render(source, &test_settings()));

            assert!(!lines[0].plain().starts_with("mermaid:"), "{source:?}: {}", lines[0].plain());
        }
    }

    #[test]
    fn text_too_wide_for_the_width_falls_back_to_source() {
        let settings = Settings { width: 12, ..test_settings() };

        let lines = lines(render(FLOWCHART, &settings));

        assert!(lines.iter().all(|line| line.width() <= 12));
    }

    #[test]
    fn garbage_never_panics() {
        let garbage = [
            "",
            " ",
            "graph",
            "graph TD\n  A[",
            "sequenceDiagram\n  ->>",
            "pie\n  \"x\": -1",
            "%%{init: ",
            "\u{0}\u{1b}[31m",
            "graph TD; A-->A-->A-->A",
        ];
        let truncated = (0..FLOWCHART.len()).filter(|end| FLOWCHART.is_char_boundary(*end)).map(|end| &FLOWCHART[..end]);

        for source in garbage.into_iter().chain(truncated) {
            for settings in [test_settings(), with_cell()] {
                let block = render(source, &settings);
                if let Block::Lines(lines) = block {
                    assert!(lines.iter().all(|line| line.width() <= settings.width), "{source:?}");
                }
            }
        }
    }
}
