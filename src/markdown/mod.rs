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

use self::context::Context;
use self::layout::Spacing;
use crate::document::{Block, Document, Settings};

const FRONT_MATTER_DELIMITER: &str = "---";

/// Renders GitHub-flavoured Markdown into a document at `settings.width`; images and links are never fetched.
pub fn render(source: &str, settings: &Settings) -> Document {
    let arena = Arena::new();
    let root = parse_document(&arena, source, &options());
    let context = Context::new(settings);
    let (notes, body): (Vec<&AstNode>, Vec<&AstNode>) = root.children().partition(|node| is_footnote_definition(node));
    let body = layout::stack(body.into_iter().map(|node| block(node, &context)), Spacing::Loose);
    let blocks = layout::stack([body, footnote::section(&notes, &context)], Spacing::Loose);
    Document { blocks }
}

fn options() -> Options<'static> {
    let mut options = Options::default();
    options.extension.table = true;
    options.extension.strikethrough = true;
    options.extension.tasklist = true;
    options.extension.autolink = true;
    options.extension.footnotes = true;
    options.extension.alerts = true;
    options.extension.front_matter_delimiter = Some(FRONT_MATTER_DELIMITER.to_owned());
    options
}

fn is_footnote_definition<'a>(node: &'a AstNode<'a>) -> bool {
    matches!(node.data().value, NodeValue::FootnoteDefinition(_))
}

/// The blocks inside a container, rendered in `context` and spaced as the container says.
fn children<'a>(node: &'a AstNode<'a>, context: &Context, spacing: Spacing) -> Vec<Block> {
    layout::stack(node.children().map(|child| block(child, context)), spacing)
}

fn block<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Block> {
    match &node.data().value {
        NodeValue::Paragraph => layout::lines(crate::text::wrap(&inline::spans(node, context.text, context), context.width(), &[])),
        NodeValue::Heading(heading) => layout::lines(heading::render(node, heading.level, context)),
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

/// The first span, at 80 columns, whose text contains `needle`.
#[cfg(test)]
fn span_with(source: &str, needle: &str) -> crate::document::Span {
    let document = render(source, &crate::theme::test_settings());
    let spans = document.blocks.into_iter().flat_map(|block| match block {
        Block::Lines(lines) => lines.into_iter().flat_map(|line| line.spans).collect(),
        Block::Picture(_) => Vec::new(),
    });
    spans.into_iter().find(|span| span.text.contains(needle)).unwrap_or_else(|| panic!("no span contains {needle:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Line;
    use crate::theme::MRK_DARK;

    /// The showcase without its fenced blocks, which belong to `code` and `mermaid`.
    fn showcase_without_fences() -> String {
        let source = include_str!("../../tests/fixtures/showcase.md");
        source.split("```").step_by(2).collect()
    }

    const EVERYTHING: &str = "# A heading long enough to wrap at twenty columns\n\n## Second level with a verylongunbreakableword\n\nText with `a long inline code span` and a [link](https://example.com/a/very/long/path) and ![an image](i.png).\n\n- item\n  - nested item with more words\n    1. ordered deeper\n       > quoted inside a list, long enough to wrap\n\n> [!CAUTION]\n> An alert with a long body that wraps.\n\n| a | b | c |\n|---|:-:|--:|\n| one cell with many words | two | three |\n\n<div class=\"x\">html block that is long enough to wrap around</div>\n\n---\n\nNote[^n].\n\n[^n]: A footnote that is long enough to wrap.\n";

    fn lines(source: &str, width: usize) -> Vec<Line> {
        let settings = Settings { width, ..crate::theme::test_settings() };
        render(source, &settings)
            .blocks
            .into_iter()
            .flat_map(|block| match block {
                Block::Lines(lines) => lines,
                Block::Picture(_) => Vec::new(),
            })
            .collect()
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
        for width in [20, 40, 80] {
            for source in [EVERYTHING.to_owned(), showcase_without_fences()] {
                let too_wide: Vec<String> = lines(&source, width).iter().filter(|line| line.width() > width).map(Line::plain).collect();

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
        let muted = Some(MRK_DARK.palette.muted);

        assert_eq!(plain_at("<details>\n<summary>Hi</summary>\n</details>", 80), "<details>\n<summary>Hi</summary>\n</details>\n");
        assert_eq!(span_with("<details>\n</details>", "<details>").style.fg, muted);
        assert_eq!(span_with("a <kbd>b</kbd> c", "<kbd>").style.fg, muted);
    }

    #[test]
    fn soft_breaks_join_and_hard_breaks_split() {
        assert_eq!(plain_at("one\ntwo  \nthree\\\nfour", 80), "one two\nthree\nfour\n");
    }

    #[test]
    fn front_matter_is_muted_lines_then_a_rule() {
        insta::assert_snapshot!(plain_at("---\ntitle: Hello\ntags: [a, b]\n---\n\nBody", 30));
        assert_eq!(span_with("---\ntitle: Hello\n---\n", "Hello").style.fg, Some(MRK_DARK.palette.muted));
    }
}
