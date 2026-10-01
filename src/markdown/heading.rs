use comrak::nodes::AstNode;

use super::context::Context;
use super::inline;
use super::layout;
use crate::document::{Block, JumboTitle, Line, Picture, Span, Style};
use crate::{raster, text};

const H1_RULE: &str = "━";
const H2_BAR: &str = "▍ ";
const JUMBO_ROWS: u16 = 2;

pub(super) fn render<'a>(node: &'a AstNode<'a>, level: u8, context: &Context) -> Vec<Block> {
    let palette = context.palette();
    let lines = match level {
        1 => return h1(node, context),
        2 => h2(node, context),
        3 => plain(node, Style::fg(palette.h3).bold(), context),
        4 | 5 => plain(node, Style::fg(palette.text).bold(), context),
        _ => plain(node, Style::fg(palette.muted).bold(), context),
    };
    layout::lines(lines)
}

fn plain<'a>(node: &'a AstNode<'a>, style: Style, context: &Context) -> Vec<Line> {
    text::wrap(&inline::spans(node, style, context), context.width(), &[])
}

fn h1<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Block> {
    let palette = context.palette();
    let spans = inline::spans(node, Style::fg(palette.h1).bold(), context);
    let title = jumbo(&spans, context).unwrap_or_else(|| layout::lines(text::wrap(&spans, context.width(), &[])));
    let rule = Line::new(vec![Span::new(H1_RULE.repeat(context.width()), Style::fg(palette.subtle))]);
    title.into_iter().chain(layout::lines(vec![rule])).collect()
}

/// The title two rows tall, wrapped at half the width since it is drawn twice as wide; `None` when the settings do not
/// ask for it, the heading sits inside a container, beside its bars, or a line cannot be drawn.
fn jumbo(spans: &[Span], context: &Context) -> Option<Vec<Block>> {
    let jumbo_title = context.settings.jumbo_title.filter(|_| context.nesting == 0)?;
    let lines = text::wrap(spans, context.width() / 2, &[]);
    match jumbo_title {
        JumboTitle::DoubleHeight => Some(vec![Block::DoubleHeight(lines)]),
        JumboTitle::Picture => lines.iter().map(|line| title_picture(line, context)).collect(),
    }
}

/// A picture of the line in the `h1` colour, its text concealed under it so it still copies as text.
fn title_picture(line: &Line, context: &Context) -> Option<Block> {
    let text = line.plain().trim_end().to_owned();
    let picture = raster::line(&text, context.palette().h1, context.settings.cell?, JUMBO_ROWS, context.width()).ok()?;
    Some(Block::Picture(Picture { concealed_text: Some(text), ..picture }))
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
    use super::*;
    use crate::document::{CellSize, Settings, plain};
    use crate::markdown::{plain_at, render, span_with};
    use crate::theme::{MRK_DARK, test_settings};

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

    fn jumbo(jumbo_title: JumboTitle, cell: Option<CellSize>) -> Settings {
        Settings { width: 30, cell, jumbo_title: Some(jumbo_title), ..test_settings() }
    }

    const CELL: CellSize = CellSize { width_px: 10, height_px: 22 };
    const TITLE: &str = "# A jumbo title that wraps\n\n## Two\n\n- # Listed";

    #[test]
    fn a_double_height_title_wraps_at_half_the_width_and_keeps_its_rule() {
        insta::assert_snapshot!(plain(&render(TITLE, &jumbo(JumboTitle::DoubleHeight, None))));
    }

    #[test]
    fn a_double_height_title_keeps_the_h1_style() {
        let Block::DoubleHeight(lines) = &render("# T", &jumbo(JumboTitle::DoubleHeight, None)).blocks[0] else { panic!("double height") };

        assert_eq!(lines[0].spans[0].style, Style::fg(MRK_DARK.palette.h1).bold());
    }

    #[test]
    fn a_picture_title_is_two_rows_per_line_over_its_concealed_text() {
        let document = render(TITLE, &jumbo(JumboTitle::Picture, Some(CELL)));
        let pictures: Vec<&Picture> =
            document.blocks.iter().filter_map(|block| if let Block::Picture(picture) = block { Some(picture) } else { None }).collect();

        assert_eq!(
            pictures.iter().map(|picture| picture.concealed_text.as_deref()).collect::<Vec<_>>(),
            [Some("A jumbo title"), Some("that wraps")]
        );
        assert!(pictures.iter().all(|picture| picture.rows == JUMBO_ROWS && usize::from(picture.cols) <= 30));
        assert!(plain(&document).contains("\n• Listed\n"), "{}", plain(&document));
    }

    #[test]
    fn a_picture_title_without_a_cell_size_is_a_normal_heading() {
        let settings = jumbo(JumboTitle::Picture, None);

        assert_eq!(plain(&render(TITLE, &settings)), plain(&render(TITLE, &Settings { jumbo_title: None, ..settings.clone() })));
    }
}
