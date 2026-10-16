use mermaid_text::RenderOptions;

use super::DiagramError;
use crate::document::{Line, Rgb, Span, Style};
use crate::text::{TAB, display_width};
use crate::theme::Palette;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Frame,
    Pointer,
    Label,
}

/// The diagram as box-drawing text no wider than `width`: frames in `muted`, arrows in `accent`, labels in `text`.
pub fn render(source: &str, width: usize, palette: &Palette) -> Result<Vec<Line>, DiagramError> {
    let drawing = draw(source, width)?;
    let lines: Vec<&str> = drawing.trim_end().lines().collect();
    if lines.is_empty() {
        return Err(DiagramError::Empty);
    }
    if lines.iter().any(|line| display_width(line) > width) {
        return Err(DiagramError::TooWide(width));
    }

    Ok(lines.into_iter().map(|line| coloured(line, palette)).collect())
}

fn draw(source: &str, width: usize) -> Result<String, DiagramError> {
    let options = RenderOptions { max_width: Some(width), max_width_strict: true, ..RenderOptions::default() };
    match mermaid_text::render_with_options(source, &options) {
        Ok(drawing) => Ok(drawing.replace('\t', TAB)),
        Err(error) => Err(DiagramError::Invalid(error.to_string())),
    }
}

fn glyph(character: char) -> Glyph {
    match character {
        '\u{2500}'..='\u{257f}' => Glyph::Frame,
        '\u{2190}'..='\u{21ff}' | '\u{2580}'..='\u{25ff}' => Glyph::Pointer,
        _ => Glyph::Label,
    }
}

fn colour(glyph: Glyph, palette: &Palette) -> Rgb {
    match glyph {
        Glyph::Frame => palette.muted,
        Glyph::Pointer => palette.accent,
        Glyph::Label => palette.text,
    }
}

fn coloured(line: &str, palette: &Palette) -> Line {
    let runs = line.chars().fold(Vec::<(Glyph, String)>::new(), |runs, character| {
        let kind = if character == ' ' { runs.last().map_or(Glyph::Label, |(kind, _)| *kind) } else { glyph(character) };
        append(runs, kind, character)
    });
    Line::new(runs.into_iter().map(|(kind, text)| Span::new(text, Style::fg(colour(kind, palette)))).collect())
}

fn append(mut runs: Vec<(Glyph, String)>, kind: Glyph, character: char) -> Vec<(Glyph, String)> {
    match runs.last_mut() {
        Some((last, text)) if *last == kind => text.push(character),
        _ => runs.push((kind, character.to_string())),
    }
    runs
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::theme::MRK_DARK;

    const FLOWCHART: &str = "graph LR\n  A[Build] --> B[Test] --> C[Deploy]";

    fn spans_in(lines: &[Line], color: Rgb) -> String {
        lines.iter().flat_map(|line| &line.spans).filter(|span| span.style.fg == Some(color)).map(|span| span.text.as_str()).collect()
    }

    #[test]
    fn frames_arrows_and_labels_take_their_colours() {
        let palette = MRK_DARK.palette;
        let lines = render(FLOWCHART, 80, &palette).unwrap();

        assert!(spans_in(&lines, palette.muted).contains('─'));
        assert!(spans_in(&lines, palette.accent).contains('▸'));
        assert!(spans_in(&lines, palette.text).contains("Deploy"));
    }

    #[test]
    fn drawing_never_exceeds_the_width() {
        let source = "graph LR\n  A[Alpha] --> B[Bravo] --> C[Charlie] --> D[Delta] --> E[Echo] --> F[Foxtrot]";

        for width in [20, 40, 60, 80, 120] {
            match render(source, width, &MRK_DARK.palette) {
                Ok(lines) => assert!(lines.iter().all(|line| line.width() <= width), "width {width}"),
                Err(error) => assert!(matches!(error, DiagramError::TooWide(_) | DiagramError::Invalid(_)), "{error:?}"),
            }
        }
    }

    #[test]
    fn sequence_diagrams_draw() {
        let lines = render("sequenceDiagram\n  Alice->>Bob: Hello", 80, &MRK_DARK.palette).unwrap();

        assert!(lines.iter().any(|line| line.plain().contains("Alice")));
    }

    #[test]
    fn distinct_subgraphs_still_draw() {
        let source = "graph TD\n subgraph outer\n subgraph inner [Inner box]\n a --> b\n end\n end\n subgraph other\n c\n end";

        let lines = render(source, 80, &MRK_DARK.palette).unwrap();

        assert!(lines.iter().any(|line| line.plain().contains("Inner box")));
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(matches!(render("not a diagram at all", 80, &MRK_DARK.palette), Err(DiagramError::Invalid(_))));
    }
}
