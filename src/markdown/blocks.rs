use std::borrow::Cow;

use comrak::nodes::{AstNode, NodeCodeBlock, NodeValue};

use super::context::Context;
use super::layout::{self, Spacing};
use super::{footnote, list, raw};
use crate::document::Block;

/// What a section is in the Markdown, so a consumer can pair and label sections without parsing again.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum SectionKind {
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

/// One top-level section of the document, its blocks as `render` draws them, with the source lines it came from.
///
/// `blocks` holds no blank line before or after the content; it is more than one `Block` only when a
/// container (a list item, a quote) holds a picture. Lines are 1-based and inclusive.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Section {
    pub first_line: usize,
    pub last_line: usize,
    pub kind: SectionKind,
    pub blocks: Vec<Block>,
}

/// A top-level node's sections and the spacing between them: one section, or one per list item.
pub(super) struct Group {
    pub spacing: Spacing,
    pub sections: Vec<Section>,
}

impl Group {
    fn single(section: Section) -> Self {
        Self { spacing: Spacing::Loose, sections: vec![section] }
    }

    /// The group as `render` stacks it.
    pub fn stacked(self) -> Vec<Block> {
        layout::stack(self.sections.into_iter().map(|section| section.blocks), self.spacing)
    }
}

fn is_footnote_definition<'a>(node: &'a AstNode<'a>) -> bool {
    matches!(node.data().value, NodeValue::FootnoteDefinition(_))
}

fn code_kind(code: &NodeCodeBlock) -> SectionKind {
    match raw::language(code) {
        Some(language) if language == raw::MERMAID => SectionKind::Mermaid,
        language => SectionKind::Code { language },
    }
}

/// `None` for a node the enabled Markdown extensions never put at the top level.
fn kind<'a>(node: &'a AstNode<'a>) -> Option<SectionKind> {
    let kind = match &node.data().value {
        NodeValue::FrontMatter(_) => SectionKind::FrontMatter,
        NodeValue::Heading(heading) => SectionKind::Heading { level: heading.level },
        NodeValue::Item(_) => SectionKind::ListItem { task: None },
        NodeValue::TaskItem(task) => SectionKind::ListItem { task: Some(task.symbol.is_some()) },
        NodeValue::BlockQuote => SectionKind::Quote,
        NodeValue::Alert(_) => SectionKind::Alert,
        NodeValue::CodeBlock(code) => code_kind(code),
        NodeValue::HtmlBlock(_) => SectionKind::Html,
        NodeValue::ThematicBreak => SectionKind::Rule,
        NodeValue::Table(_) => SectionKind::Table,
        NodeValue::Paragraph => SectionKind::Paragraph,
        _ => return None,
    };
    Some(kind)
}

/// Whether each source line holds more than spaces, lines ending where comrak ends them: at `\n`, `\r\n` or a lone `\r`.
fn written_lines(source: &str) -> Vec<bool> {
    let source = if source.contains('\r') { Cow::Owned(source.replace("\r\n", "\n")) } else { Cow::Borrowed(source) };
    source.split(['\n', '\r']).map(|line| !line.trim().is_empty()).collect()
}

/// comrak ends an item that closes its list on the blank line after it; a section ends on its last written line.
fn last_written_line(first_line: usize, last_line: usize, written: &[bool]) -> usize {
    let is_written = |line: &usize| written.get(line - 1).copied().unwrap_or(false);
    (first_line..=last_line).rev().find(is_written).unwrap_or(first_line)
}

fn section<'a>(node: &'a AstNode<'a>, blocks: Vec<Block>, written: &[bool]) -> Option<Section> {
    let position = node.data().sourcepos;
    let last_line = last_written_line(position.start.line, position.end.line, written);
    Some(Section { first_line: position.start.line, last_line, kind: kind(node)?, blocks })
}

fn group<'a>(node: &'a AstNode<'a>, written: &[bool], context: &Context) -> Group {
    match &node.data().value {
        NodeValue::List(list) => {
            let items = node.children().zip(list::items(node, list, context));
            let sections = items.filter_map(|(item, blocks)| section(item, blocks, written)).collect();
            Group { spacing: list::spacing(list), sections }
        }
        _ => Group { spacing: Spacing::Loose, sections: section(node, super::block(node, context), written).into_iter().collect() },
    }
}

/// The notes section spans from the first line of the earliest definition to the last line of the latest,
/// lines between definitions included: comrak reorders the notes, so no single definition's lines would do.
fn footnotes<'a>(definitions: &[&'a AstNode<'a>], written: &[bool], context: &Context) -> Option<Group> {
    let first_line = definitions.iter().map(|definition| definition.data().sourcepos.start.line).min()?;
    let last_line = definitions.iter().map(|definition| definition.data().sourcepos.end.line).max()?;
    let last_line = last_written_line(first_line, last_line, written);
    let blocks = footnote::section(definitions, context);
    Some(Group::single(Section { first_line, last_line, kind: SectionKind::Footnotes, blocks }))
}

/// Every top-level group in reading order, the footnote definitions gathered into one group at the end.
pub(super) fn groups<'a>(root: &'a AstNode<'a>, source: &str, context: &Context) -> Vec<Group> {
    let written = written_lines(source);
    let (notes, body): (Vec<&AstNode>, Vec<&AstNode>) = root.children().partition(|node| is_footnote_definition(node));
    let body = body.into_iter().map(|node| group(node, &written, context));
    body.chain(footnotes(&notes, &written, context)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{CellSize, Line, Settings, test_settings};
    use crate::markdown::{render, render_sections};

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
                other => vec![other],
            })
            .collect()
    }

    fn outline(source: &str) -> Vec<(usize, usize, SectionKind)> {
        render_sections(source, &test_settings()).into_iter().map(|section| (section.first_line, section.last_line, section.kind)).collect()
    }

    #[test]
    fn sections_hold_what_render_draws_less_the_spacing() {
        for width in [20, 40, 80, 100] {
            for cell in [None, Some(CELL)] {
                let settings = Settings { width, cell, ..test_settings() };
                let blocks: Vec<Block> = render_sections(SHOWCASE, &settings).into_iter().flat_map(|section| section.blocks).collect();

                assert_eq!(content(blocks), content(render(SHOWCASE, &settings).blocks), "width {width}, cell {cell:?}");
            }
        }
    }

    #[test]
    fn no_section_starts_or_ends_with_a_blank_line() {
        let is_blank_edge =
            |block: &Block, edge: fn(&[Line]) -> Option<&Line>| matches!(block, Block::Lines(lines) if edge(lines) == Some(&Line::blank()));
        for section in render_sections(EVERY_KIND, &test_settings()).into_iter().chain(render_sections(SHOWCASE, &test_settings())) {
            assert!(!section.blocks.first().is_some_and(|first| is_blank_edge(first, <[Line]>::first)), "{:?}", section.kind);
            assert!(!section.blocks.last().is_some_and(|last| is_blank_edge(last, <[Line]>::last)), "{:?}", section.kind);
        }
    }

    #[test]
    fn each_section_points_at_its_source_lines() {
        assert_eq!(
            outline(EVERY_KIND),
            [
                (1, 3, SectionKind::FrontMatter),
                (5, 5, SectionKind::Heading { level: 1 }),
                (7, 8, SectionKind::Paragraph),
                (10, 11, SectionKind::ListItem { task: None }),
                (12, 12, SectionKind::ListItem { task: Some(true) }),
                (14, 16, SectionKind::Code { language: Some("rust".to_owned()) }),
                (18, 20, SectionKind::Table),
                (22, 25, SectionKind::Mermaid),
                (27, 28, SectionKind::Alert),
                (30, 30, SectionKind::Paragraph),
                (32, 32, SectionKind::Footnotes),
            ]
        );
    }

    #[test]
    fn lines_end_where_comrak_ends_them_whatever_the_line_ending() {
        for ending in ["\n", "\r\n", "\r"] {
            let source = ["- one", "  more", "", "- two", "", "", "text"].join(ending) + ending;

            assert_eq!(
                outline(&source),
                [
                    (1, 2, SectionKind::ListItem { task: None }),
                    (4, 4, SectionKind::ListItem { task: None }),
                    (7, 7, SectionKind::Paragraph)
                ],
                "{ending:?}"
            );
        }
    }

    #[test]
    fn the_notes_span_every_definition_in_source_order() {
        let source = "b[^b] a[^a]\n\n[^a]: A.\n\ntext\n\n[^b]: B.\n";

        assert_eq!(outline(source).last(), Some(&(3, 7, SectionKind::Footnotes)));
    }

    #[test]
    fn a_list_splits_into_its_items_and_other_containers_stay_whole() {
        let kinds: Vec<SectionKind> =
            outline("1. one\n2. two\n\n> quote\n>\n> more\n\n---\n\n<div>x</div>\n").into_iter().map(|(.., kind)| kind).collect();

        assert_eq!(
            kinds,
            [
                SectionKind::ListItem { task: None },
                SectionKind::ListItem { task: None },
                SectionKind::Quote,
                SectionKind::Rule,
                SectionKind::Html
            ]
        );
    }

    #[test]
    fn a_fence_without_language_is_code_without_one() {
        assert_eq!(outline("```\nx\n```\n"), [(1, 3, SectionKind::Code { language: None })]);
    }

    #[test]
    fn a_fence_names_its_language_by_its_first_word_lowercased() {
        let kinds: Vec<SectionKind> = ["Rust,ignore", "{.mermaid}", "Mermaid", "mermaid,ignore"]
            .iter()
            .flat_map(|info| outline(&format!("```{info}\ngraph TD\n```\n")))
            .map(|(.., kind)| kind)
            .collect();

        assert_eq!(
            kinds,
            [SectionKind::Code { language: Some("rust".to_owned()) }, SectionKind::Mermaid, SectionKind::Mermaid, SectionKind::Mermaid]
        );
    }

    #[test]
    fn a_mermaid_section_is_its_picture_when_the_terminal_can_draw() {
        let settings = Settings { cell: Some(CELL), ..test_settings() };
        let sections = render_sections("```mermaid\ngraph TD\n  A --> B\n```\n", &settings);

        assert!(
            matches!(sections[..], [Section { kind: SectionKind::Mermaid, ref blocks, .. }] if matches!(blocks[..], [Block::Picture(_)]))
        );
    }
}
