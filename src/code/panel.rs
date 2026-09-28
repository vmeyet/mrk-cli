use crate::document::{Line, Span, Style};
use crate::text::display_width;
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

    let room = width.saturating_sub(display_width(ELLIPSIS));
    let kept = text.chars().scan(0, |used, character| {
        *used += char_width(character);
        (*used <= room).then_some(character)
    });
    kept.chain(ELLIPSIS.chars()).collect()
}

/// Hard-wraps one source row to `width` cells, each continuation row opening with the marker; code is never re-flowed at spaces.
fn wrap(row: &[Span], width: usize, continuation: &Span) -> Vec<Vec<Span>> {
    let mut rows: Vec<Vec<Span>> = vec![Vec::new()];
    let mut used = 0;
    let mut row_start = 0;
    for (character, style) in row.iter().flat_map(|span| span.text.chars().map(|character| (character, span.style))) {
        let cells = char_width(character);
        if used > row_start && used + cells > width {
            rows.push(vec![continuation.clone()]);
            row_start = display_width(&continuation.text);
            used = row_start;
        }
        if let Some(current) = rows.last_mut() {
            append(current, character, style);
        }
        used += cells;
    }
    rows
}

fn append(row: &mut Vec<Span>, character: char, style: Style) {
    match row.last_mut() {
        Some(last) if last.style == style => last.text.push(character),
        _ => row.push(Span::new(character.to_string(), style)),
    }
}

fn char_width(character: char) -> usize {
    display_width(character.encode_utf8(&mut [0; 4]))
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
