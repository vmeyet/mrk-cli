use super::state::Pager;
use crate::document::{Line, Span, Style};
use crate::text::{cut, display_width};
use crate::theme::Palette;

const HINT: &str = "q quit · / search";
const PADDING: usize = 1;
const GAP: usize = 3;
const MIN_NAME_WIDTH: usize = 12;
const ELLIPSIS: char = '…';
const CARET: &str = "▏";

fn width(spans: &[Span]) -> usize {
    spans.iter().map(|span| display_width(&span.text)).sum()
}

fn truncated(text: &str, columns: usize) -> String {
    match columns {
        _ if display_width(text) <= columns => text.to_owned(),
        0 => String::new(),
        _ => format!("{}{ELLIPSIS}", cut(text, columns - 1).0),
    }
}

fn position(pager: &Pager) -> String {
    let rows = pager.page.rows.len();
    let percent = (pager.top + pager.height).min(rows) * 100 / rows.max(1);
    match () {
        () if rows <= pager.height => "All".to_owned(),
        () if pager.top == 0 => "Top".to_owned(),
        () if pager.top >= pager.max_top() => "Bot".to_owned(),
        () => format!("{percent}%"),
    }
}

fn middle(pager: &Pager, palette: &Palette) -> Vec<Span> {
    let on = |style: Style| style.on(palette.surface);
    let search = &pager.search;
    match (&pager.prompt, search.current) {
        (Some(typed), _) => vec![
            Span::new("/", on(Style::fg(palette.accent).bold())),
            Span::new(typed.clone(), on(Style::fg(palette.text))),
            Span::new(CARET, on(Style::fg(palette.accent))),
        ],
        (None, Some(current)) => vec![
            Span::new(format!("{}", current + 1), on(Style::fg(palette.accent).bold())),
            Span::new(format!("/{}", search.matches.len()), on(Style::fg(palette.muted))),
        ],
        (None, None) if !search.query.is_empty() => vec![Span::new("no match", on(Style::fg(palette.warning)))],
        (None, None) => Vec::new(),
    }
}

fn right(pager: &Pager, palette: &Palette, has_hint: bool) -> Vec<Span> {
    let position = Span::new(position(pager), Style::fg(palette.accent).on(palette.surface));
    let hint = has_hint.then(|| Span::new(format!("   {HINT}"), Style::fg(palette.muted).on(palette.surface)));
    std::iter::once(position).chain(hint).collect()
}

fn fill(columns: usize, palette: &Palette) -> Span {
    Span::new(" ".repeat(columns), Style::default().on(palette.surface))
}

fn needed(middle: &[Span], right: &[Span]) -> usize {
    let middle_width = if middle.is_empty() { 0 } else { width(middle) + GAP };
    2 * PADDING + GAP + middle_width + width(right)
}

/// Centred in the row, unless that would crowd the right side, and never over the name.
fn middle_start(name_end: usize, middle_width: usize, right_width: usize, columns: usize) -> usize {
    let centred = columns.saturating_sub(middle_width) / 2;
    let latest = columns.saturating_sub(PADDING + right_width + GAP + middle_width);
    centred.min(latest).max(name_end + GAP)
}

/// The spans cut to `columns` cells.
fn clipped(spans: Vec<Span>, columns: usize) -> Vec<Span> {
    let mut left = columns;
    spans
        .into_iter()
        .map(|span| {
            let (text, _) = cut(&span.text, left);
            left -= display_width(text);
            Span { text: text.to_owned(), ..span }
        })
        .filter(|span| !span.text.is_empty())
        .collect()
}

/// The last row: the file name on the left, the prompt or the match count in the middle, the position and a key hint
/// on the right, all on `surface`, exactly `columns` wide. The hint goes first when the row is short, then the name shrinks.
pub fn bar(pager: &Pager, name: &str, palette: &Palette, columns: usize) -> Line {
    let middle = middle(pager, palette);
    let has_hint = needed(&middle, &right(pager, palette, true)) + MIN_NAME_WIDTH <= columns;
    let right = right(pager, palette, has_hint);
    let name =
        Span::new(truncated(name, columns.saturating_sub(needed(&middle, &right))), Style::fg(palette.text).on(palette.surface).bold());

    let name_end = PADDING + width(std::slice::from_ref(&name));
    let middle_start = middle_start(name_end, width(&middle), width(&right), columns);
    let middle_end = if middle.is_empty() { name_end } else { middle_start + width(&middle) };
    let right_start = columns.saturating_sub(PADDING + width(&right)).max(middle_end + GAP);
    let before_middle = if middle.is_empty() { 0 } else { middle_start - name_end };

    let spans = [
        vec![fill(PADDING, palette), name, fill(before_middle, palette)],
        middle,
        vec![fill(right_start - middle_end, palette)],
        right,
        vec![fill(columns, palette)],
    ];
    Line::new(clipped(spans.concat(), columns))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Line;
    use crate::terminal::pager::keys::Action;
    use crate::terminal::pager::page::{Page, Row};
    use crate::theme::MRK_DARK;

    fn pager(rows: usize, height: usize) -> Pager {
        let rows = (0..rows).map(|index| Row::Line(Line::new(vec![Span::plain(format!("row {index}"))]))).collect();
        Pager::new(Page { rows, pictures: Vec::new() }, height)
    }

    fn shown(pager: &Pager, columns: usize) -> String {
        let line = bar(pager, "notes/architecture.md", &MRK_DARK.palette, columns);
        assert_eq!(line.width(), columns, "{:?}", line.plain());
        format!("|{}|", line.plain())
    }

    #[test]
    fn the_bar_at_several_widths() {
        let searched =
            [Action::StartSearch, Action::Type('1'), Action::Confirm, Action::NextMatch].iter().fold(pager(100, 20), Pager::apply);
        let typing = [Action::StartSearch, Action::Type('r'), Action::Type('o')].iter().fold(pager(100, 20), Pager::apply);
        let lines: Vec<String> = [80, 50, 30, 12, 3]
            .into_iter()
            .flat_map(|columns| [shown(&pager(100, 20), columns), shown(&searched, columns), shown(&typing, columns)])
            .collect();

        insta::assert_snapshot!(lines.join("\n"));
    }

    #[test]
    fn the_position_reads_top_bottom_percent_or_all() {
        let long = pager(100, 20);

        assert_eq!(position(&long), "Top");
        assert_eq!(position(&Pager { top: 30, ..long.clone() }), "50%");
        assert_eq!(position(&Pager { top: 80, ..long }), "Bot");
        assert_eq!(position(&pager(5, 20)), "All");
    }

    #[test]
    fn a_query_without_match_says_so() {
        let searched = [Action::StartSearch, Action::Type('z'), Action::Confirm].iter().fold(pager(10, 5), Pager::apply);

        assert!(shown(&searched, 60).contains("no match"));
    }

    #[test]
    fn every_cell_sits_on_the_surface() {
        let palette = MRK_DARK.palette;

        assert!(bar(&pager(10, 5), "a.md", &palette, 40).spans.iter().all(|span| span.style.bg == Some(palette.surface)));
    }
}
