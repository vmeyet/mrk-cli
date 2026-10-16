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
use std::time::{Duration, Instant};

pub use self::budget::Load;
use self::cache::Recent;
pub(crate) use self::catch::is_catching;
use crate::document::{Block, CellSize, Line, Picture, Settings};
use crate::raster::{RasterError, Svg};

/// How long a diagram may take to lay out, both engines together, before it shows as source; debug builds run the
/// engines about ten times slower, and tests render through them.
const DEADLINE: Duration = Duration::from_millis(if cfg!(debug_assertions) { 20_000 } else { 2_000 });

static DRAWINGS: Recent<Result<Arc<Svg>, DiagramError>> = Recent::new();
static SLOW_AS_TEXT: Recent<()> = Recent::new();

/// Why a diagram is not drawn; the source fallback shows it in its note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagramError {
    Oversized,
    UnsafeSubgraphs,
    OverBudget(Load),
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

/// A ```mermaid block: a picture when `settings.cell` is set and the diagram renders, text otherwise.
pub fn render(source: &str, settings: &Settings) -> Block {
    render_within(source, settings, DEADLINE)
}

fn render_within(source: &str, settings: &Settings, deadline: Duration) -> Block {
    drawn(source, settings, Instant::now() + deadline).unwrap_or_else(|error| as_source(source, &error, settings))
}

fn drawn(source: &str, settings: &Settings, deadline: Instant) -> Result<Block, DiagramError> {
    budget::admit(source)?;
    match settings.cell.and_then(|cell| picture(source, cell, settings, deadline).ok()) {
        Some(picture) => Ok(Block::Picture(picture)),
        None => as_text(source, settings, deadline),
    }
}

/// Parsed, laid out and turned into a tree once per source and palette, a timeout included: only the fit and the
/// raster depend on the width.
fn picture(source: &str, cell: CellSize, settings: &Settings, deadline: Instant) -> Result<Picture, DiagramError> {
    let palette = settings.theme.palette;
    let drawing = DRAWINGS.get_or(source, &palette, || {
        let source = source.to_owned();
        worker::within(deadline, move || Ok(Arc::new(Svg::parse(&svg::render(&budget::parse(&source)?, &palette))?)))
    })?;
    let (width, height) = drawing.size();
    let frame = frame::fit(width, height, svg::FONT_SIZE_PX, cell, settings.width).ok_or(DiagramError::TooLarge)?;
    let png = drawing.draw(frame.scale, frame.width_px, frame.height_px)?;
    let alt = kind::describe(source).to_owned();
    Ok(Picture { png, cols: frame.cols, rows: frame.rows, alt, indent: Line::blank(), concealed_text: None })
}

/// mermaid-text lays out for the width, so it runs again on every resize, unless it once ran out of time.
fn as_text(source: &str, settings: &Settings, deadline: Instant) -> Result<Block, DiagramError> {
    budget::admit_text(source)?;
    let palette = settings.theme.palette;
    if SLOW_AS_TEXT.find(source, &palette).is_some() {
        return Err(DiagramError::TimedOut);
    }
    let (owned, width) = (source.to_owned(), settings.width);
    let drawn = worker::within(deadline, move || text::render(&owned, width, &palette));
    if matches!(drawn, Err(DiagramError::TimedOut)) {
        SLOW_AS_TEXT.keep(source, &palette, ());
    }
    drawn.map(Block::Lines)
}

fn as_source(source: &str, error: &DiagramError, settings: &Settings) -> Block {
    Block::Lines(source::render(source, &error.to_string(), settings.width, &settings.theme.palette))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::time::{Duration, Instant};

    use super::*;
    use crate::document::Line;
    use crate::theme::test_settings;

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
    fn diagrams_too_costly_to_lay_out_are_refused_at_once() {
        let costly = [
            (tangled(100, 500), "mermaid: over 160 statements, too many to lay out"),
            (tangled(1000, 3500), "mermaid: over 160 statements, too many to lay out"),
            (format!("classDiagram\n{}", "  A --> B\n".repeat(40)), "mermaid: over 32 statements, too many to lay out"),
            (format!("sequenceDiagram\n{}", "  a ->> b: hi\n".repeat(1000)), "mermaid: over 160 statements, too many to lay out"),
            (format!("gitGraph\n{}", "  commit\n".repeat(3000)), "mermaid: over 160 statements, too many to lay out"),
        ];

        for (source, note) in &costly {
            assert!(source.len() <= budget::MAX_SOURCE_BYTES, "{note}: {} bytes", source.len());
            for settings in [test_settings(), with_cell()] {
                let started = Instant::now();
                let lines = lines(render(source, &settings));

                assert!(started.elapsed() < Duration::from_secs(1), "{note}: {:?}", started.elapsed());
                assert_eq!(lines[0].plain(), *note);
            }
        }
    }

    #[test]
    fn a_diagram_out_of_time_shows_as_source_and_stays_so_on_a_resize() {
        let source = tangled(20, 40);

        let out_of_time = lines(render_within(&source, &with_cell(), Duration::ZERO));
        let resized = lines(render_within(&source, &Settings { width: 60, ..with_cell() }, DEADLINE));

        assert_eq!(out_of_time[0].plain(), "mermaid: too slow to lay out");
        assert_eq!(resized[0].plain(), "mermaid: too slow to lay out");
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
