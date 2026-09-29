use crate::document::{Line, Span, Style};
use crate::text::{WIDEST_GRAPHEME, cut, display_width};
use crate::theme::Palette;

const PADDING: &str = "  ";
const CONTINUATION: &str = "↪ ";
const ELLIPSIS: &str = "…";

/// The highlighted rows framed on `surface`: a padded top row carrying the label, the rows wrapped to fit, a padded bottom row.
pub fn panel(rows: &[Vec<Span>], label: Option<&str>, width: usize, palette: &Palette) -> Vec<Line> {
    let content_width = width.saturating_sub(2 * display_width(PADDING));
    let continuation = Span::new(CONTINUATION, Style::fg(palette.subtle));
    let body = rows.iter().flat_map(|row| wrap(row, content_width, &continuation));
    let framed = std::iter::once(label_row(label, content_width, palette)).chain(body).chain(std::iter::once(Vec::new()));
    framed.map(|content| pad(&content, width, palette)).collect()
}

fn label_row(label: Option<&str>, content_width: usize, palette: &Palette) -> Vec<Span> {
    let Some(label) = label else { return Vec::new() };
    let label = fit(label, content_width);
    let indent = " ".repeat(content_width.saturating_sub(display_width(&label)));
    vec![Span::plain(indent), Span::new(label, Style::fg(palette.muted))]
}

/// `text` shortened with an ellipsis when it is wider than `width`.
fn fit(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }

    let (kept, _) = cut(text, width.saturating_sub(display_width(ELLIPSIS)));
    format!("{kept}{ELLIPSIS}")
}

/// Hard-wraps one source row to `width` cells, each continuation row opening with the marker; code is never re-flowed at spaces.
fn wrap(row: &[Span], width: usize, continuation: &Span) -> Vec<Vec<Span>> {
    let mut rows: Vec<Vec<Span>> = vec![Vec::new()];
    let mut used = 0;
    let mut row_start = 0;
    for span in row {
        let mut rest = span.text.as_str();
        while !rest.is_empty() {
            let free = width.saturating_sub(used);
            let (head, tail) = cut(rest, if used == row_start { free.max(WIDEST_GRAPHEME) } else { free });
            if head.is_empty() {
                rows.push(vec![continuation.clone()]);
                row_start = display_width(&continuation.text);
                used = row_start;
                continue;
            }
            if let Some(current) = rows.last_mut() {
                append(current, head, span.style);
            }
            used += display_width(head);
            rest = tail;
        }
    }
    rows
}

fn append(row: &mut Vec<Span>, text: &str, style: Style) {
    match row.last_mut() {
        Some(last) if last.style == style => last.text.push_str(text),
        _ => row.push(Span::new(text, style)),
    }
}

fn pad(content: &[Span], width: usize, palette: &Palette) -> Line {
    let used = display_width(PADDING) + content.iter().map(|span| display_width(&span.text)).sum::<usize>();
    let fill = Span::plain(" ".repeat(width.saturating_sub(used)));
    let spans = std::iter::once(Span::plain(PADDING)).chain(content.iter().cloned()).chain(std::iter::once(fill));
    Line::new(spans.filter(|span| !span.text.is_empty()).map(|span| on_surface(span, palette)).collect())
}

fn on_surface(span: Span, palette: &Palette) -> Span {
    let style = Style { fg: span.style.fg.or(Some(palette.text)), bg: Some(palette.surface), ..span.style };
    Span { style, ..span }
}
