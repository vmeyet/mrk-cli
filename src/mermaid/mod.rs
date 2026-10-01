mod frame;
mod kind;
mod source;
mod svg;
mod text;

use std::fmt;

use crate::document::{Block, CellSize, Line, Picture, Settings};
use crate::raster::{RasterError, Svg};

/// Mermaid blocks above this size are shown as source (`specs/02-security.md` rule 4).
const MAX_SOURCE_BYTES: usize = 64 * 1024;

/// Why a diagram is not drawn; the source fallback shows it in its note.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagramError {
    Oversized,
    UnsafeSubgraphs,
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
            Self::Oversized => write!(formatter, "diagram over 64 KiB"),
            Self::UnsafeSubgraphs => write!(formatter, "subgraph id reused or nested too deep"),
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
    if source.len() > MAX_SOURCE_BYTES {
        return as_source(source, &DiagramError::Oversized, settings);
    }

    match settings.cell.and_then(|cell| picture(source, cell, settings).ok()) {
        Some(picture) => Block::Picture(picture),
        None => as_text(source, settings).unwrap_or_else(|error| as_source(source, &error, settings)),
    }
}

fn picture(source: &str, cell: CellSize, settings: &Settings) -> Result<Picture, DiagramError> {
    let drawing = Svg::parse(&svg::render(source, &settings.theme.palette)?, source)?;
    let (width, height) = drawing.size();
    let frame = frame::fit(width, height, svg::FONT_SIZE_PX, cell, settings.width).ok_or(DiagramError::TooLarge)?;
    let png = drawing.draw(frame.scale, frame.width_px, frame.height_px)?;
    let alt = kind::describe(source).to_owned();
    Ok(Picture { png, cols: frame.cols, rows: frame.rows, alt, indent: Line::blank(), concealed_text: None })
}

fn as_text(source: &str, settings: &Settings) -> Result<Block, DiagramError> {
    text::render(source, settings.width, &settings.theme.palette).map(Block::Lines)
}

fn as_source(source: &str, error: &DiagramError, settings: &Settings) -> Block {
    Block::Lines(source::render(source, &error.to_string(), settings.width, &settings.theme.palette))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

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
    fn a_subgraph_inside_itself_is_shown_as_source() {
        let lines = lines(render("graph TD\n subgraph a\n subgraph a\n end\n end", &test_settings()));

        assert_eq!(lines[0].plain(), "mermaid: subgraph id reused or nested too deep");
        assert_eq!(lines[1].plain(), "graph TD");
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
