use crate::document::{Line, Span, Style};
use crate::text::{cut, display_width, hard_wrap};
use crate::theme::Palette;

const PADDING: &str = "  ";
const CONTINUATION: &str = "↪ ";
const ELLIPSIS: &str = "…";

/// The highlighted rows framed on `surface`: a padded top row carrying the label, the rows hard-wrapped to fit, each
/// continuation opening with a marker, and a padded bottom row.
pub fn panel(rows: &[Vec<Span>], label: Option<&str>, width: usize, palette: &Palette) -> Vec<Line> {
    let content_width = width.saturating_sub(2 * display_width(PADDING));
    let continuation = [Span::new(CONTINUATION, Style::fg(palette.subtle))];
    let body = rows.iter().flat_map(|row| hard_wrap(row, content_width, &continuation));
    let framed = std::iter::once(label_row(label, content_width, palette)).chain(body).chain(std::iter::once(Line::blank()));
    framed.map(|content| pad(content, width, palette)).collect()
}

fn label_row(label: Option<&str>, content_width: usize, palette: &Palette) -> Line {
    let Some(label) = label else { return Line::blank() };
    let label = fit(label, content_width);
    let indent = " ".repeat(content_width.saturating_sub(display_width(&label)));
    Line::new(vec![Span::plain(indent), Span::new(label, Style::fg(palette.muted))])
}

/// `text` shortened with an ellipsis when it is wider than `width`.
fn fit(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }

    let (kept, _) = cut(text, width.saturating_sub(display_width(ELLIPSIS)));
    format!("{kept}{ELLIPSIS}")
}

fn pad(content: Line, width: usize, palette: &Palette) -> Line {
    let fill = Span::plain(" ".repeat(width.saturating_sub(display_width(PADDING) + content.width())));
    let spans = std::iter::once(Span::plain(PADDING)).chain(content.spans).chain(std::iter::once(fill));
    Line::new(spans.filter(|span| !span.text.is_empty()).map(|span| on_surface(span, palette)).collect())
}

fn on_surface(span: Span, palette: &Palette) -> Span {
    let style = Style { fg: span.style.fg.or(Some(palette.text)), bg: Some(palette.surface), ..span.style };
    Span { style, ..span }
}
