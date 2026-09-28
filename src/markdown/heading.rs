use comrak::nodes::AstNode;

use super::context::Context;
use super::inline;
use super::layout;
use crate::document::{Line, Span, Style};
use crate::text;

const H1_RULE: &str = "━";
const H2_BAR: &str = "▍ ";

pub(super) fn render<'a>(node: &'a AstNode<'a>, level: u8, context: &Context) -> Vec<Line> {
    let palette = context.palette();
    match level {
        1 => h1(node, context),
        2 => h2(node, context),
        3 => plain(node, Style::fg(palette.h3).bold(), context),
        4 | 5 => plain(node, Style::fg(palette.text).bold(), context),
        _ => plain(node, Style::fg(palette.muted).bold(), context),
    }
}

fn plain<'a>(node: &'a AstNode<'a>, style: Style, context: &Context) -> Vec<Line> {
    text::wrap(&inline::spans(node, style, context), context.width(), &[])
}

fn h1<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Line> {
    let palette = context.palette();
    let title = plain(node, Style::fg(palette.h1).bold(), context);
    let rule = Line::new(vec![Span::new(H1_RULE.repeat(context.width()), Style::fg(palette.subtle))]);
    title.into_iter().chain(std::iter::once(rule)).collect()
}

fn h2<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Line> {
    let palette = context.palette();
    let bar_width = text::display_width(H2_BAR);
    let spans = inline::spans(node, Style::fg(palette.h2).bold(), context);
    let title = text::wrap(&spans, context.width().saturating_sub(bar_width), &[]);
    layout::prefix_lines(title, &[Span::new(H2_BAR, Style::fg(palette.h2))], &layout::blank(bar_width))
}

#[cfg(test)]
mod tests {
    use crate::markdown::{plain_at, span_with};
    use crate::theme::MRK_DARK;

    #[test]
    fn levels_one_to_six() {
        insta::assert_snapshot!(plain_at("# One\n\n## Two\n\n### Three\n\n#### Four\n\n##### Five\n\n###### Six", 24));
    }

    #[test]
    fn h2_wraps_under_its_bar() {
        insta::assert_snapshot!(plain_at("## A second level title that wraps", 20));
    }

    #[test]
    fn each_level_takes_its_palette_colour() {
        let palette = MRK_DARK.palette;
        let cases =
            [("# T", palette.h1), ("## T", palette.h2), ("### T", palette.h3), ("#### T", palette.text), ("###### T", palette.muted)];
        for (source, color) in cases {
            let title = span_with(source, "T");

            assert_eq!((title.style.fg, title.style.bold), (Some(color), true), "{source}");
        }
    }

    #[test]
    fn h1_rule_is_subtle_and_spans_the_width() {
        let rule = span_with("# T", "━");

        assert_eq!(rule.style.fg, Some(MRK_DARK.palette.subtle));
        assert_eq!(rule.text.chars().count(), 80);
    }

    #[test]
    fn no_hash_is_left_on_screen() {
        assert!(!plain_at("# One\n\n## Two\n\n### Three", 80).contains('#'));
    }
}
