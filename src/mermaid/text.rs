use std::borrow::Cow;

use mermaid_text::RenderOptions;
use mermaid_text::detect::DiagramKind;
use mermaid_text::parser;

use super::DiagramError;
use super::catch::caught;
use crate::document::{Line, Rgb, Span, Style};
use crate::text::{display_width, expand_tabs, rewritten};
use crate::theme::Palette;

const ESC: char = '\u{1b}';
const BEL: char = '\u{7}';
/// The string terminator, ESC then a backslash.
const ST: &str = "\u{1b}\\";

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

/// The participants of a sequence diagram or the branches of a git graph as mermaid-text reads them, every row drawn
/// across all of them; `None` for any other diagram or one it cannot read.
pub fn lanes(source: &str) -> Option<usize> {
    caught(|| match mermaid_text::detect::detect(source).ok()? {
        DiagramKind::Sequence => parser::sequence::parse(source).ok().map(|diagram| diagram.participants.len()),
        DiagramKind::GitGraph => parser::git_graph::parse(source).ok().map(|graph| graph.branches.len()),
        _ => None,
    })
    .ok()
    .flatten()
}

fn draw(source: &str, width: usize) -> Result<String, DiagramError> {
    let options = RenderOptions { max_width: Some(width), max_width_strict: true, ..RenderOptions::default() };
    match mermaid_text::render_with_options(source, &options) {
        Ok(drawing) => Ok(expand_tabs(&without_escapes(&drawing), 0).into_owned()),
        Err(error) => Err(DiagramError::Invalid(error.to_string())),
    }
}

/// mermaid-text wraps a node a `click` line names in an OSC 8 hyperlink, colour off or not, and lays out its box
/// without it: each sequence goes whole, as its width count skips it, where `sanitize` would leave all but the ESC
/// on screen.
fn without_escapes(drawing: &str) -> Cow<'_, str> {
    rewritten(drawing, &[ESC], |escape| (Cow::Borrowed(""), escape_length(escape)))
}

/// The bytes of the sequence `text` opens on with its ESC: a CSI up to its final byte, an OSC up to its BEL or ST, to
/// the end when unterminated; any other ESC alone.
fn escape_length(text: &str) -> usize {
    let opened = &text[ESC.len_utf8()..];
    let body = if let Some(csi) = opened.strip_prefix('[') {
        csi.find(|character| ('\u{40}'..='\u{7e}').contains(&character)).map_or(csi.len(), |end| end + 1)
    } else if let Some(osc) = opened.strip_prefix(']') {
        osc.find([BEL, ESC]).map_or(osc.len(), |end| end + 1 + usize::from(osc[end..].starts_with(ST)))
    } else {
        return ESC.len_utf8();
    };
    ESC.len_utf8() + 1 + body
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

    #[test]
    fn a_clicked_node_shows_its_label_alone_in_an_aligned_box() {
        let lines = render("flowchart LR\n a --> b\n click a \"https://x\"", 80, &MRK_DARK.palette).unwrap();

        let plain: Vec<String> = lines.iter().map(Line::plain).collect();
        assert_eq!(plain, ["┌───┐     ┌───┐", "│ a │────▸│ b │", "└───┘     └───┘"]);
    }

    #[test]
    fn every_escape_sequence_goes_whole() {
        let drawing = "a\u{1b}]8;;https://x\u{1b}\\b\u{1b}]8;;\u{7}c\u{1b}[38;2;1;2;3md\u{1b}xe\u{1b}]8;;unterminated";

        assert_eq!(without_escapes(drawing), "abcdxe");
        assert_eq!(without_escapes("\u{1b}[31"), "");
    }
}
