use super::context::Context;
use super::layout;
use crate::document::{Line, Span, Style};
use crate::text;

const RULE: &str = "─";
const CENTRE: &str = " ◆ ";
/// The only front matter fence comrak is told about: YAML's.
pub(super) const FRONT_MATTER_DELIMITER: &str = "---";

/// `───── ◆ ─────` across the width.
pub(super) fn thematic(context: &Context) -> Line {
    let width = context.width();
    let border = Style::fg(context.palette().subtle);
    let sides = width.saturating_sub(text::display_width(CENTRE));
    if sides == 0 {
        return full(context);
    }
    let left = sides / 2;
    Line::new(vec![
        Span::new(RULE.repeat(left), border),
        Span::new(CENTRE, Style::fg(context.palette().muted)),
        Span::new(RULE.repeat(sides - left), border),
    ])
}

/// A plain rule across the whole width.
pub(super) fn full(context: &Context) -> Line {
    short(context.width(), context)
}

/// A plain rule of `width` cells, never wider than the context.
pub(super) fn short(width: usize, context: &Context) -> Line {
    Line::new(vec![Span::new(RULE.repeat(width.min(context.width())), Style::fg(context.palette().subtle))])
}

/// The lines between the fences in `muted`, blank ones left out, then a rule.
pub(super) fn front_matter(source: &str, context: &Context) -> Vec<Line> {
    let muted = Style::fg(context.palette().muted);
    let entries = front_matter_body(source).filter(|line| !line.trim().is_empty());
    let lines = entries.flat_map(|line| entry(line, muted, context.width()));
    lines.chain(std::iter::once(full(context))).collect()
}

/// A front matter line behind its own indent (YAML nests by it), up to half the width; it wraps two cells further in.
fn entry(line: &str, muted: Style, width: usize) -> Vec<Line> {
    let value = line.trim_start();
    let indent_width = text::display_width(&line[..line.len() - value.len()]).min(width / 2);
    let lines = text::wrap(&[Span::new(value, muted)], width.saturating_sub(indent_width), &[Span::plain("  ")]);
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
    use crate::markdown::{plain_at, span_with};
    use crate::theme::MRK_DARK;

    #[test]
    fn thematic_break_spans_the_width_around_a_diamond() {
        assert_eq!(plain_at("---", 13), "───── ◆ ─────\n");
        assert_eq!(plain_at("***", 12), "──── ◆ ─────\n");
    }

    #[test]
    fn thematic_break_is_subtle() {
        assert_eq!(span_with("---", "─").style.fg, Some(MRK_DARK.palette.subtle));
    }
}
