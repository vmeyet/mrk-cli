use super::context::Context;
use crate::document::{Line, Span, Style};
use crate::text;

const RULE: &str = "─";
const CENTRE: &str = " ◆ ";
const FRONT_MATTER_FENCES: [&str; 2] = ["---", "+++"];

/// `───── ◆ ─────` across the width.
pub(super) fn thematic(context: &Context) -> Line {
    let width = context.width();
    let border = Style::fg(context.palette().subtle);
    let sides = width.saturating_sub(text::display_width(CENTRE));
    let left = sides / 2;
    if sides == 0 {
        return full(context);
    }
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

/// The `key: value` lines between the fences in `muted`, keys bold, then a rule.
pub(super) fn front_matter(source: &str, context: &Context) -> Vec<Line> {
    let muted = Style::fg(context.palette().muted);
    let entries = source.lines().map(str::trim_end).filter(|line| !line.trim().is_empty() && !FRONT_MATTER_FENCES.contains(&line.trim()));
    let lines = entries.flat_map(|line| text::wrap(&entry(line, muted), context.width(), &[Span::plain("  ")]));
    lines.chain(std::iter::once(full(context))).collect()
}

fn entry(line: &str, muted: Style) -> Vec<Span> {
    match line.split_once(':') {
        Some((key, value)) if !key.starts_with(' ') => vec![Span::new(format!("{key}:"), muted.bold()), Span::new(value.to_owned(), muted)],
        _ => vec![Span::new(line.to_owned(), muted)],
    }
}

#[cfg(test)]
mod tests {
    use crate::markdown::{plain_at, span_with};
    use crate::theme::MRK_DARK;

    #[test]
    fn thematic_break_spans_the_width_around_a_diamond() {
        assert_eq!(plain_at("---", 11), "──── ◆ ────\n");
        assert_eq!(plain_at("***", 12), "──── ◆ ─────\n");
    }

    #[test]
    fn thematic_break_is_subtle() {
        assert_eq!(span_with("---", "─").style.fg, Some(MRK_DARK.palette.subtle));
    }
}
