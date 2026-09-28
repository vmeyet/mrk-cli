use comrak::nodes::{AstNode, NodeCodeBlock, NodeValue};

use super::context::Context;
use super::layout::{self, Spacing};
use super::{footnote, list, raw};
use crate::document::Block;

/// What a source block is in the Markdown, so a consumer can pair and label blocks without parsing again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockKind {
    FrontMatter,
    Heading {
        level: u8,
    },
    Paragraph,
    /// One top-level item of a list; `task` is `Some(done)` for a task item.
    ListItem {
        task: Option<bool>,
    },
    Quote,
    Alert,
    Code {
        language: Option<String>,
    },
    Mermaid,
    Table,
    Rule,
    Html,
    Footnotes,
}

/// One top-level block of the document, rendered as `render` draws it, with the source lines it came from.
///
/// `blocks` holds no blank line before or after the content; it is more than one `Block` only when a
/// container (a list item, a quote) holds a picture. Lines are 1-based and inclusive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceBlock {
    pub first_line: usize,
    pub last_line: usize,
    pub kind: BlockKind,
    pub blocks: Vec<Block>,
}

/// A top-level node's source blocks and the spacing between them: one block, or one per list item.
pub(super) struct Section {
    pub spacing: Spacing,
    pub blocks: Vec<SourceBlock>,
}

impl Section {
    fn single(block: SourceBlock) -> Self {
        Self { spacing: Spacing::Loose, blocks: vec![block] }
    }

    /// The section as `render` stacks it.
    pub fn stacked(self) -> Vec<Block> {
        layout::stack(self.blocks.into_iter().map(|block| block.blocks), self.spacing)
    }
}

fn is_footnote_definition<'a>(node: &'a AstNode<'a>) -> bool {
    matches!(node.data().value, NodeValue::FootnoteDefinition(_))
}

fn code_kind(code: &NodeCodeBlock) -> BlockKind {
    match raw::language(code) {
        Some(raw::MERMAID) => BlockKind::Mermaid,
        language => BlockKind::Code { language: language.map(str::to_owned) },
    }
}

fn kind<'a>(node: &'a AstNode<'a>) -> BlockKind {
    match &node.data().value {
        NodeValue::FrontMatter(_) => BlockKind::FrontMatter,
        NodeValue::Heading(heading) => BlockKind::Heading { level: heading.level },
        NodeValue::Item(_) => BlockKind::ListItem { task: None },
        NodeValue::TaskItem(task) => BlockKind::ListItem { task: Some(task.symbol.is_some()) },
        NodeValue::BlockQuote | NodeValue::MultilineBlockQuote(_) => BlockKind::Quote,
        NodeValue::Alert(_) => BlockKind::Alert,
        NodeValue::CodeBlock(code) => code_kind(code),
        NodeValue::HtmlBlock(_) => BlockKind::Html,
        NodeValue::ThematicBreak => BlockKind::Rule,
        NodeValue::Table(_) => BlockKind::Table,
        _ => BlockKind::Paragraph,
    }
}

/// comrak ends an item that closes its list on the blank line after it; a block ends on its last written line.
fn last_written_line(first_line: usize, last_line: usize, source_lines: &[&str]) -> usize {
    let is_written = |line: &usize| source_lines.get(line - 1).is_some_and(|text| !text.trim().is_empty());
    (first_line..=last_line).rev().find(is_written).unwrap_or(first_line)
}

fn source_block<'a>(node: &'a AstNode<'a>, blocks: Vec<Block>, source_lines: &[&str]) -> SourceBlock {
    let position = node.data().sourcepos;
    let last_line = last_written_line(position.start.line, position.end.line, source_lines);
    SourceBlock { first_line: position.start.line, last_line, kind: kind(node), blocks }
}

fn section<'a>(node: &'a AstNode<'a>, source_lines: &[&str], context: &Context) -> Section {
    match &node.data().value {
        NodeValue::List(list) => {
            let items = node.children().zip(list::items(node, list, context));
            let blocks = items.map(|(item, blocks)| source_block(item, blocks, source_lines)).collect();
            Section { spacing: list::spacing(list), blocks }
        }
        _ => Section::single(source_block(node, super::block(node, context), source_lines)),
    }
}

/// The notes section spans from the first line of the earliest definition to the last line of the latest,
/// lines between definitions included: comrak reorders the notes, so no single definition's lines would do.
fn footnotes<'a>(definitions: &[&'a AstNode<'a>], source_lines: &[&str], context: &Context) -> Option<Section> {
    let first_line = definitions.iter().map(|definition| definition.data().sourcepos.start.line).min()?;
    let last_line = definitions.iter().map(|definition| definition.data().sourcepos.end.line).max()?;
    let last_line = last_written_line(first_line, last_line, source_lines);
    let blocks = footnote::section(definitions, context);
    Some(Section::single(SourceBlock { first_line, last_line, kind: BlockKind::Footnotes, blocks }))
}

/// Every top-level section in reading order, the footnote definitions gathered into one section at the end.
pub(super) fn sections<'a>(root: &'a AstNode<'a>, source: &str, context: &Context) -> Vec<Section> {
    let source_lines: Vec<&str> = source.lines().collect();
    let (notes, body): (Vec<&AstNode>, Vec<&AstNode>) = root.children().partition(|node| is_footnote_definition(node));
    let body = body.into_iter().map(|node| section(node, &source_lines, context));
    body.chain(footnotes(&notes, &source_lines, context)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{CellSize, Line, Settings};
    use crate::markdown::{render, render_blocks};
    use crate::theme::test_settings;

    const SHOWCASE: &str = include_str!("../../tests/fixtures/showcase.md");
    const CELL: CellSize = CellSize { width_px: 10, height_px: 22 };
    const EVERY_KIND: &str = "---\ntitle: T\n---\n\n# Heading\n\nA paragraph\nover two lines\n\n- item one\n  - nested\n- [x] item two\n\n```rust\nfn a() {}\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n```mermaid\ngraph TD\n  A --> B\n```\n\n> [!NOTE]\n> Noted.\n\nNote[^n].\n\n[^n]: The note.\n";

    /// Every line and picture, blank separator lines dropped: the content without its spacing.
    fn content(blocks: Vec<Block>) -> Vec<Block> {
        blocks
            .into_iter()
            .flat_map(|block| match block {
                Block::Lines(lines) => {
                    lines.into_iter().filter(|line| *line != Line::blank()).map(|line| Block::Lines(vec![line])).collect()
                }
                picture @ Block::Picture(_) => vec![picture],
            })
            .collect()
    }

    fn outline(source: &str) -> Vec<(usize, usize, BlockKind)> {
        render_blocks(source, &test_settings()).into_iter().map(|block| (block.first_line, block.last_line, block.kind)).collect()
    }

    #[test]
    fn blocks_hold_what_render_draws_less_the_spacing() {
        for width in [20, 40, 80, 100] {
            for cell in [None, Some(CELL)] {
                let settings = Settings { width, cell, ..test_settings() };
                let blocks: Vec<Block> = render_blocks(SHOWCASE, &settings).into_iter().flat_map(|block| block.blocks).collect();

                assert!(content(blocks) == content(render(SHOWCASE, &settings).blocks), "width {width}, cell {cell:?}");
            }
        }
    }

    #[test]
    fn no_block_starts_or_ends_with_a_blank_line() {
        let is_blank_edge =
            |block: &Block, edge: fn(&[Line]) -> Option<&Line>| matches!(block, Block::Lines(lines) if edge(lines) == Some(&Line::blank()));
        for block in render_blocks(EVERY_KIND, &test_settings()).into_iter().chain(render_blocks(SHOWCASE, &test_settings())) {
            assert!(!block.blocks.first().is_some_and(|first| is_blank_edge(first, <[Line]>::first)), "{:?}", block.kind);
            assert!(!block.blocks.last().is_some_and(|last| is_blank_edge(last, <[Line]>::last)), "{:?}", block.kind);
        }
    }

    #[test]
    fn each_block_points_at_its_source_lines() {
        assert_eq!(
            outline(EVERY_KIND),
            [
                (1, 3, BlockKind::FrontMatter),
                (5, 5, BlockKind::Heading { level: 1 }),
                (7, 8, BlockKind::Paragraph),
                (10, 11, BlockKind::ListItem { task: None }),
                (12, 12, BlockKind::ListItem { task: Some(true) }),
                (14, 16, BlockKind::Code { language: Some("rust".to_owned()) }),
                (18, 20, BlockKind::Table),
                (22, 25, BlockKind::Mermaid),
                (27, 28, BlockKind::Alert),
                (30, 30, BlockKind::Paragraph),
                (32, 32, BlockKind::Footnotes),
            ]
        );
    }

    #[test]
    fn the_notes_span_every_definition_in_source_order() {
        let source = "b[^b] a[^a]\n\n[^a]: A.\n\ntext\n\n[^b]: B.\n";

        assert_eq!(outline(source).last(), Some(&(3, 7, BlockKind::Footnotes)));
    }

    #[test]
    fn a_list_splits_into_its_items_and_other_containers_stay_whole() {
        let kinds: Vec<BlockKind> =
            outline("1. one\n2. two\n\n> quote\n>\n> more\n\n---\n\n<div>x</div>\n").into_iter().map(|(.., kind)| kind).collect();

        assert_eq!(
            kinds,
            [BlockKind::ListItem { task: None }, BlockKind::ListItem { task: None }, BlockKind::Quote, BlockKind::Rule, BlockKind::Html]
        );
    }

    #[test]
    fn a_fence_without_language_is_code_without_one() {
        assert_eq!(outline("```\nx\n```\n"), [(1, 3, BlockKind::Code { language: None })]);
    }

    #[test]
    fn a_mermaid_block_is_its_picture_when_the_terminal_can_draw() {
        let settings = Settings { cell: Some(CELL), ..test_settings() };
        let blocks = render_blocks("```mermaid\ngraph TD\n  A --> B\n```\n", &settings);

        assert!(
            matches!(blocks[..], [SourceBlock { kind: BlockKind::Mermaid, blocks: ref inner, .. }] if matches!(inner[..], [Block::Picture(_)]))
        );
    }
}
