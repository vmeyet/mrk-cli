use comrak::nodes::{AstNode, NodeTable, NodeValue, TableAlignment};

use super::context::Context;
use super::inline;
use crate::document::{Line, Span, Style};
use crate::text;

/// A squeezed column keeps room for a short word; below that the table renders as records.
const MIN_COLUMN_WIDTH: usize = 3;
/// `│ ` before each cell and ` │` after the last: three cells of frame per column, plus one.
const FRAME_PER_COLUMN: usize = 3;
const RECORD_INDENT: &str = "  ";

type Cell = Vec<Span>;

struct Row {
    cells: Vec<Cell>,
    is_header: bool,
}

struct WrappedRow {
    cells: Vec<Vec<Line>>,
    is_header: bool,
}

/// Box-drawing corners and junctions for one horizontal border.
struct Border {
    left: &'static str,
    junction: &'static str,
    right: &'static str,
}

const TOP: Border = Border { left: "╭", junction: "┬", right: "╮" };
const MIDDLE: Border = Border { left: "├", junction: "┼", right: "┤" };
const BOTTOM: Border = Border { left: "╰", junction: "┴", right: "╯" };

/// A rounded box with the header in bold `accent`; cells wrap when the table is wider than the context,
/// and a table with too many columns to fit renders as one `header: value` record per row.
pub(super) fn render<'a>(node: &'a AstNode<'a>, table: &NodeTable, context: &Context) -> Vec<Line> {
    let rows = rows(node, table.alignments.len(), context);
    match column_widths(&rows, table.alignments.len(), context.width()) {
        Some(widths) => boxed(&rows, &widths, &table.alignments, context),
        None => records(&rows, context),
    }
}

fn rows<'a>(node: &'a AstNode<'a>, columns: usize, context: &Context) -> Vec<Row> {
    let header = Style::fg(context.palette().accent).bold();
    node.children()
        .map(|row| {
            let is_header = matches!(row.data().value, NodeValue::TableRow(true));
            let style = if is_header { header } else { context.text };
            let cells = row.children().map(|cell| inline::spans(cell, style, context)).chain(std::iter::repeat_with(Vec::new));
            Row { cells: cells.take(columns).collect(), is_header }
        })
        .collect()
}

fn cell_width(cell: &Cell) -> usize {
    cell.iter().map(|span| text::display_width(&span.text)).sum::<usize>().max(1)
}

/// Each column as wide as its widest cell when that fits; otherwise the room is shared, narrow columns keeping
/// their natural width and wide ones splitting the rest evenly. `None` when even that cannot fit.
fn column_widths(rows: &[Row], columns: usize, width: usize) -> Option<Vec<usize>> {
    let natural: Vec<usize> = (0..columns).map(|column| rows.iter().map(|row| cell_width(&row.cells[column])).max().unwrap_or(1)).collect();
    let room = width.checked_sub(FRAME_PER_COLUMN * columns + 1)?;
    if natural.iter().sum::<usize>() <= room {
        return Some(natural);
    }
    if room < MIN_COLUMN_WIDTH * columns {
        return None;
    }
    let mut by_width: Vec<usize> = (0..columns).collect();
    by_width.sort_by_key(|column| natural[*column]);
    let mut widths = vec![0; columns];
    let mut remaining = room;
    for (placed, column) in by_width.into_iter().enumerate() {
        let share = remaining / (columns - placed);
        widths[column] = natural[column].min(share);
        remaining -= widths[column];
    }
    Some(widths)
}

fn boxed(rows: &[Row], widths: &[usize], alignments: &[TableAlignment], context: &Context) -> Vec<Line> {
    let border = Style::fg(context.palette().subtle);
    let wrapped: Vec<WrappedRow> = rows.iter().map(|row| wrap_row(row, widths)).collect();
    let widths = fitted_widths(&wrapped, widths.len());
    let (header, body): (Vec<&WrappedRow>, Vec<&WrappedRow>) = wrapped.iter().partition(|row| row.is_header);
    let separator = (!body.is_empty()).then(|| rule(&MIDDLE, &widths, border));
    let header_lines = header.into_iter().flat_map(|row| row_lines(row, &widths, alignments, border));
    let body_lines = body.into_iter().flat_map(|row| row_lines(row, &widths, alignments, border));
    let top = std::iter::once(rule(&TOP, &widths, border));
    let bottom = std::iter::once(rule(&BOTTOM, &widths, border));
    top.chain(header_lines).chain(separator).chain(body_lines).chain(bottom).collect()
}

fn wrap_row(row: &Row, widths: &[usize]) -> WrappedRow {
    WrappedRow {
        cells: row.cells.iter().zip(widths).map(|(cell, width)| text::wrap(cell, *width, &[])).collect(),
        is_header: row.is_header,
    }
}

/// Each column only as wide as its widest wrapped line, so a squeezed column gives back what wrapping left over.
fn fitted_widths(rows: &[WrappedRow], columns: usize) -> Vec<usize> {
    (0..columns).map(|column| rows.iter().flat_map(|row| row.cells[column].iter().map(Line::width)).max().unwrap_or(0).max(1)).collect()
}

fn rule(edges: &Border, widths: &[usize], border: Style) -> Line {
    let segments: Vec<String> = widths.iter().map(|width| "─".repeat(width + 2)).collect();
    Line::new(vec![Span::new(format!("{}{}{}", edges.left, segments.join(edges.junction), edges.right), border)])
}

fn row_lines(row: &WrappedRow, widths: &[usize], alignments: &[TableAlignment], border: Style) -> Vec<Line> {
    let wrapped = &row.cells;
    let height = wrapped.iter().map(Vec::len).max().unwrap_or(1);
    (0..height)
        .map(|index| {
            let cells = wrapped.iter().zip(widths).zip(alignments).map(|((lines, width), alignment)| {
                let line = lines.get(index).cloned().unwrap_or_default();
                aligned(line, *width, *alignment)
            });
            let joined = cells.enumerate().flat_map(|(column, cell)| {
                let separator = if column == 0 { "│ " } else { " │ " };
                std::iter::once(Span::new(separator, border)).chain(cell)
            });
            Line::new(joined.chain(std::iter::once(Span::new(" │", border))).collect())
        })
        .collect()
}

fn aligned(line: Line, width: usize, alignment: TableAlignment) -> Vec<Span> {
    let free = width.saturating_sub(line.width());
    let before = match alignment {
        TableAlignment::Right => free,
        TableAlignment::Center => free / 2,
        TableAlignment::Left | TableAlignment::None => 0,
    };
    let padding = |cells: usize| Span::plain(" ".repeat(cells));
    std::iter::once(padding(before))
        .chain(line.spans)
        .chain(std::iter::once(padding(free - before)))
        .filter(|span| !span.text.is_empty())
        .collect()
}

fn records(rows: &[Row], context: &Context) -> Vec<Line> {
    let Some((header, body)) = rows.split_first() else { return Vec::new() };
    let names: Vec<String> = header.cells.iter().map(|cell| cell.iter().map(|span| span.text.as_str()).collect()).collect();
    let name_style = Style::fg(context.palette().accent).bold();
    let records = body.iter().map(|row| {
        let fields = names.iter().zip(&row.cells).flat_map(|(name, cell)| {
            let spans: Vec<Span> = std::iter::once(Span::new(format!("{name}: "), name_style)).chain(cell.iter().cloned()).collect();
            text::wrap(&spans, context.width(), &[Span::plain(RECORD_INDENT)])
        });
        fields.collect::<Vec<Line>>()
    });
    records.collect::<Vec<_>>().join(&Line::blank())
}

#[cfg(test)]
mod tests {
    use crate::markdown::{plain_at, span_with};
    use crate::theme::MRK_DARK;

    const TABLE: &str =
        "| Name | Left | Center | Right |\n|------|:-----|:------:|------:|\n| mrk | a | b | 1 |\n| glow | longer cell text | c | 22 |";

    #[test]
    fn natural_width_with_alignment() {
        insta::assert_snapshot!(plain_at(TABLE, 80));
    }

    #[test]
    fn cells_wrap_when_the_table_is_too_wide() {
        insta::assert_snapshot!(plain_at(TABLE, 34));
    }

    #[test]
    fn too_many_columns_render_as_records() {
        let header = (1..=12).map(|column| format!("c{column}")).collect::<Vec<_>>().join(" | ");
        let row = (1..=12).map(|column| format!("v{column}")).collect::<Vec<_>>().join(" | ");
        let source = format!("| {header} |\n|{}\n| {row} |\n| {row} |", "---|".repeat(12));

        insta::assert_snapshot!(plain_at(&source, 30));
    }

    #[test]
    fn header_is_bold_accent_and_borders_subtle() {
        let palette = MRK_DARK.palette;
        let header = span_with(TABLE, "Name");

        assert_eq!((header.style.fg, header.style.bold), (Some(palette.accent), true));
        assert_eq!(span_with(TABLE, "╭").style.fg, Some(palette.subtle));
    }

    #[test]
    fn missing_cells_are_empty() {
        insta::assert_snapshot!(plain_at("| a | b |\n|---|---|\n| only |", 40));
    }
}
