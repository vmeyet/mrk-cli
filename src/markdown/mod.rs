mod blocks;
mod context;
mod footnote;
mod heading;
mod inline;
mod layout;
mod list;
mod quote;
mod raw;
mod rule;
mod table;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

use self::blocks::Section;
pub use self::blocks::{BlockKind, SourceBlock};
use self::context::Context;
use self::layout::Spacing;
use crate::document::{Block, Document, Settings, Span, Style};
use crate::text;

/// Renders GitHub-flavoured Markdown into a document at `settings.width`; images and links are never fetched.
pub fn render(source: &str, settings: &Settings) -> Document {
    let blocks = layout::stack(sections(source, settings).into_iter().map(Section::stacked), Spacing::Loose);
    Document { blocks }
}

/// The document's top-level blocks, each with the source lines it came from, for a consumer that diffs or pairs blocks.
/// A list gives one block per top-level item, nested items staying inside theirs; the footnote definitions are one
/// last block. No blank line separates or surrounds them: `render` puts one between blocks and between loose items.
pub fn render_blocks(source: &str, settings: &Settings) -> Vec<SourceBlock> {
    let blocks = sections(source, settings).into_iter().flat_map(|section| section.blocks);
    blocks.filter(|block| !layout::is_empty(&block.blocks)).collect()
}

fn sections(source: &str, settings: &Settings) -> Vec<Section> {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options());
    blocks::sections(root, source, &Context::new(settings))
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    options.extension.alerts = true;
    options.extension.front_matter_delimiter = Some(rule::FRONT_MATTER_DELIMITER.to_owned());
    options
}

/// The blocks inside a container, rendered in `context` and spaced as the container says.
fn children<'a>(node: &'a AstNode<'a>, context: &Context, spacing: Spacing) -> Vec<Block> {
    layout::stack(node.children().map(|child| block(child, context)), spacing)
}

/// A container's blocks at its own width, with no room left to indent them: `lead` (a list marker) opens its first
/// paragraph, or stands on a line of its own above anything else.
/// `restyle` changes the blocks, never the lead.
fn unindented<'a>(node: &'a AstNode<'a>, lead: &[Span], context: &Context, spacing: Spacing, restyle: fn(Style) -> Style) -> Vec<Block> {
    let opening = node.first_child().filter(|child| matches!(child.data().value, NodeValue::Paragraph));
    let rest = node.children().skip(usize::from(opening.is_some())).map(|child| block(child, context));
    let rest = layout::restyle(layout::stack(rest, spacing), restyle);
    match opening {
        Some(paragraph) => {
            let text = inline::spans(paragraph, context.text, context).into_iter().map(|span| Span { style: restyle(span.style), ..span });
            let led: Vec<Span> = lead.iter().cloned().chain(text).collect();
            layout::stack([layout::lines(text::wrap(&led, context.width(), &[])), rest], spacing)
        }
        None => layout::stack([layout::lines(text::wrap(lead, context.width(), &[])), rest], Spacing::Tight),
    }
}

fn block<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Block> {
    match &node.data().value {
        NodeValue::Paragraph => layout::lines(text::wrap(&inline::spans(node, context.text, context), context.width(), &[])),
        NodeValue::Heading(heading) => heading::render(node, heading.level, context),
        NodeValue::List(list) => list::render(node, list, context),
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => quote::render(node, context),
        NodeValue::Alert(alert) => quote::alert(node, alert, context),
        NodeValue::CodeBlock(code) => raw::code(code, context),
        NodeValue::HtmlBlock(html) => layout::lines(raw::html(&html.literal, context)),
        NodeValue::ThematicBreak => layout::lines(vec![rule::thematic(context)]),
        NodeValue::Table(table) => layout::lines(table::render(node, table, context)),
        NodeValue::FrontMatter(source) => layout::lines(rule::front_matter(source, context)),
        _ => children(node, context, Spacing::Loose),
    }
}

#[cfg(test)]
fn plain_at(source: &str, width: usize) -> String {
    crate::document::plain(&render(source, &Settings { width, ..crate::theme::test_settings() }))
}

/// `plain_at` 40 columns wide on a terminal that draws pictures.
#[cfg(test)]
fn plain_with_pictures(source: &str) -> String {
    let cell = Some(crate::document::CellSize { width_px: 10, height_px: 22 });
    crate::document::plain(&render(source, &Settings { width: 40, cell, ..crate::theme::test_settings() }))
}

/// The spans of every line at 80 columns, pictures left out.
#[cfg(test)]
fn styled(source: &str) -> Vec<Vec<Span>> {
    let document = render(source, &crate::theme::test_settings());
    document.blocks.iter().flat_map(Block::lines).map(|line| line.spans.clone()).collect()
}

/// `document::ansi` of the source at `width` columns.
#[cfg(all(test, feature = "cli"))]
fn ansi_at(source: &str, width: usize) -> String {
    crate::document::ansi(&render(source, &Settings { width, ..crate::theme::test_settings() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Line, MIN_WIDTH, Style};
    use crate::theme::MRK_DARK;

    /// The showcase without its fenced blocks, which belong to `code` and `mermaid`.
    fn showcase_without_fences() -> String {
        let source = include_str!("../../tests/fixtures/showcase.md");
        source.split("```").step_by(2).collect()
    }

    const EVERYTHING: &str = "# A heading long enough to wrap at twenty columns\n\n## Second level with a verylongunbreakableword\n\nText with `a long inline code span` and a [link](https://example.com/a/very/long/path) and ![an image](i.png).\n\n- item\n  - nested item with more words\n    1. ordered deeper\n       > quoted inside a list, long enough to wrap\n\n> [!CAUTION]\n> An alert with a long body that wraps.\n\n| a | b | c |\n|---|:-:|--:|\n| one cell with many words | two | three |\n\n<div class=\"x\">html block that is long enough to wrap around</div>\n\n---\n\nNote[^n].\n\n[^n]: A footnote that is long enough to wrap.\n";

    fn lines(source: &str, width: usize) -> Vec<Line> {
        let settings = Settings { width, ..crate::theme::test_settings() };
        render(source, &settings).blocks.iter().flat_map(Block::lines).cloned().collect()
    }

    #[test]
    fn showcase_at_80_columns() {
        insta::assert_snapshot!(plain_at(&showcase_without_fences(), 80));
    }

    #[test]
    fn showcase_at_40_columns() {
        insta::assert_snapshot!(plain_at(&showcase_without_fences(), 40));
    }

    #[test]
    fn no_line_exceeds_the_width() {
        let showcase = include_str!("../../tests/fixtures/showcase.md");
        let zero_width_words = format!("{} \u{200b} b \u{ad} c \u{301} d\n", "a".repeat(20));
        for width in MIN_WIDTH..=60 {
            for source in [EVERYTHING, showcase, &zero_width_words] {
                let too_wide: Vec<String> = lines(source, width).iter().filter(|line| line.width() > width).map(Line::plain).collect();

                assert!(too_wide.is_empty(), "at {width} columns: {too_wide:#?}");
            }
        }
    }

    #[test]
    fn blocks_are_one_blank_line_apart_with_none_around() {
        let lines = lines(EVERYTHING, 40);
        let is_blank = |line: &Line| line.plain().trim().is_empty();

        assert!(!is_blank(&lines[0]));
        assert!(!is_blank(&lines[lines.len() - 1]));
        assert!(lines.windows(2).all(|pair| !(is_blank(&pair[0]) && is_blank(&pair[1]))));
    }

    #[test]
    fn a_width_under_the_minimum_lays_out_at_the_minimum() {
        assert_eq!(plain_at(EVERYTHING, 3), plain_at(EVERYTHING, MIN_WIDTH));
    }

    #[test]
    fn empty_source_is_an_empty_document() {
        assert_eq!(render("", &crate::theme::test_settings()), Document::default());
    }

    #[test]
    fn hostile_nesting_renders_without_overflowing_the_stack() {
        let sources =
            ["> ".repeat(100_000), "- ".repeat(20_000), format!("{}a{}", "*".repeat(50_000), "*".repeat(50_000)), "[".repeat(50_000)];
        for source in sources {
            assert!(lines(&source, 40).iter().all(|line| line.width() <= 40));
        }
    }

    #[test]
    fn html_is_shown_as_muted_literal_text() {
        let (muted, text) = (Style::fg(MRK_DARK.palette.muted), Style::fg(MRK_DARK.palette.text));

        assert_eq!(plain_at("<details>\n<summary>Hi</summary>\n</details>", 80), "<details>\n<summary>Hi</summary>\n</details>\n");
        assert_eq!(styled("<details>\n</details>"), [[Span::new("<details>", muted)], [Span::new("</details>", muted)]]);
        assert_eq!(
            styled("a <kbd>b</kbd> c"),
            [[Span::new("a ", text), Span::new("<kbd>", muted), Span::new("b", text), Span::new("</kbd>", muted), Span::new(" c", text)]]
        );
    }

    #[test]
    fn soft_breaks_join_and_hard_breaks_split() {
        assert_eq!(plain_at("one\ntwo  \nthree\\\nfour", 80), "one two\nthree\nfour\n");
    }

    #[test]
    fn front_matter_is_muted_lines_then_a_rule() {
        insta::assert_snapshot!(plain_at("---\ntitle: Hello\ntags: [a, b]\n---\n\nBody", 30));
        assert_eq!(
            styled("---\ntitle: Hello\n---\n"),
            [
                [Span::new("title: Hello", Style::fg(MRK_DARK.palette.muted))],
                [Span::new("─".repeat(80), Style::fg(MRK_DARK.palette.subtle))]
            ]
        );
    }

    #[test]
    fn front_matter_keeps_every_line_between_its_fences() {
        let source = "---\nnote: |\n  ---\n  +++\n---\n\n\nBody";

        assert_eq!(plain_at(source, 30), format!("note: |\n  ---\n  +++\n{}\n\nBody\n", "─".repeat(30)));
    }

    #[test]
    fn a_front_matter_indent_takes_at_most_half_the_width() {
        let source = format!("---\n{}deep: value\n---\n", " ".repeat(100));

        assert_eq!(plain_at(&source, 20).lines().next(), Some("          deep:"));
    }
}
