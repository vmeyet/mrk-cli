use unicode_width::UnicodeWidthChar;

use crate::document::{Line, Span, Style};
use crate::theme::Palette;

/// A muted `mermaid: <reason>` note, then the source itself, every line cut to `width` cells.
pub fn render(source: &str, reason: &str, width: usize, palette: &Palette) -> Vec<Line> {
    let style = Style::fg(palette.muted);
    let note = format!("mermaid: {reason}");
    let note_lines = cut(&note, width).into_iter().take(1);
    let source_lines = source.lines().flat_map(|line| cut(&line.replace('\t', "    "), width));
    note_lines.chain(source_lines).map(|text| Line::new(vec![Span::new(text, style)])).collect()
}

fn cut(text: &str, width: usize) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    text.chars()
        .filter(|character| character.width().is_some_and(|cells| cells <= width))
        .fold(Vec::new(), |pieces, character| push_character(pieces, character, width))
}

fn push_character(mut pieces: Vec<String>, character: char, width: usize) -> Vec<String> {
    let cells = character.width().unwrap_or(0);
    match pieces.last_mut() {
        Some(piece) if crate::text::display_width(piece) + cells <= width => piece.push(character),
        _ => pieces.push(character.to_string()),
    }
    pieces
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::theme::MRK_DARK;

    #[test]
    fn note_then_source_in_muted() {
        let lines = render("graph TD\n  A --> B", "unknown diagram", 80, &MRK_DARK.palette);

        let texts: Vec<String> = lines.iter().map(Line::plain).collect();
        assert_eq!(texts, ["mermaid: unknown diagram", "graph TD", "  A --> B"]);
        assert!(lines.iter().flat_map(|line| &line.spans).all(|span| span.style.fg == Some(MRK_DARK.palette.muted)));
    }

    #[test]
    fn long_and_wide_lines_are_cut_to_the_width() {
        let source = format!("{}\n{}\n\n\tx", "a".repeat(25), "漢".repeat(7));

        let lines = render(&source, &"reason ".repeat(10), 10, &MRK_DARK.palette);

        assert!(lines.iter().all(|line| line.width() <= 10));
        assert_eq!(lines.iter().map(Line::plain).filter(|text| text.starts_with('a')).count(), 3);
        assert_eq!(lines.iter().filter(|line| line.plain().starts_with('漢')).map(Line::width).collect::<Vec<_>>(), [10, 4]);
    }
}
