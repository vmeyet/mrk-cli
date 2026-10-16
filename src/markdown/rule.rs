use super::context::Context;
use super::layout;
use crate::document::{Line, Span, Style};
use crate::text;

const RULE: &str = "─";
const CENTRE: &str = " ◆ ";
/// The only front matter fence comrak is told about: YAML's.
pub(super) const FRONT_MATTER_DELIMITER: &str = "---";

/// `───── ◆ ─────` across the width, which `MIN_WIDTH` keeps wider than the centre.
pub(super) fn thematic(context: &Context) -> Line {
    let border = Style::fg(context.palette().subtle);
    let sides = context.width() - text::display_width(CENTRE);
    let left = sides / 2;
    Line::new(vec![
        Span::new(RULE.repeat(left), border).framed(),
        Span::new(CENTRE, Style::fg(context.palette().muted)).framed(),
        Span::new(RULE.repeat(sides - left), border).framed(),
    ])
}

pub(super) fn full(context: &Context) -> Line {
    short(context.width(), context)
}

pub(super) fn short(width: usize, context: &Context) -> Line {
    Line::new(vec![Span::new(RULE.repeat(width.min(context.width())), Style::fg(context.palette().subtle)).framed()])
}

/// The lines between the fences in `muted`, blank ones left out, then a rule.
pub(super) fn front_matter(source: &str, context: &Context) -> Vec<Line> {
    let muted = Style::fg(context.palette().muted);
    let entries = front_matter_body(source).filter(|line| !line.trim().is_empty());
    let lines = entries.flat_map(|line| entry(line, muted, context.width()));
    lines.chain(std::iter::once(full(context))).collect()
}

/// A front matter line behind its own indent (YAML nests by it), up to half the width, its spacing kept; it goes on
/// two cells further in.
fn entry(line: &str, muted: Style, width: usize) -> Vec<Line> {
    let value = line.trim_start();
    let indent_width = text::display_width(&line[..line.len() - value.len()]).min(width / 2);
    let lines = text::wrap_literal(&[Span::new(value, muted)], width - indent_width, &[Span::plain("  ").framed()]);
    let indent = layout::blank(indent_width);
    layout::prefix_lines(lines, &indent, &indent)
}

/// comrak's front matter is the opening fence, the body, the closing fence and the blank lines after it.
fn front_matter_body(source: &str) -> impl Iterator<Item = &str> {
    let lines = source.trim_end().lines();
    let fenced = lines.clone().count();
    lines.skip(1).take(fenced.saturating_sub(2))
}

#[cfg(test)]
mod tests {
    use crate::document::{Span, Style};
    use crate::markdown::{plain_at, styled};
    use crate::theme::MRK_DARK;

    #[test]
    fn thematic_break_spans_the_width_around_a_diamond() {
        assert_eq!(plain_at("---", 13), "───── ◆ ─────\n");
        assert_eq!(plain_at("***", 12), "──── ◆ ─────\n");
    }

    #[test]
    fn thematic_break_is_subtle_around_a_muted_diamond() {
        let palette = MRK_DARK.palette;
        let border = |cells: usize| Span::new("─".repeat(cells), Style::fg(palette.subtle)).framed();

        assert_eq!(styled("---"), [[border(38), Span::new(" ◆ ", Style::fg(palette.muted)).framed(), border(39)]]);
    }

    #[test]
    fn front_matter_keeps_the_spacing_of_its_values() {
        assert_eq!(plain_at("---\nname:    mrk\n---\n", 20).lines().next(), Some("name:    mrk"));
    }
}
