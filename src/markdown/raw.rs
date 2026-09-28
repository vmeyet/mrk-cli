use comrak::nodes::{AstNode, NodeCodeBlock};

use super::context::Context;
use super::inline;
use super::layout;
use crate::document::{Block, Line, Span, Style};
use crate::text;

const MERMAID: &str = "mermaid";

/// An HTML block as muted literal text, never interpreted.
pub(super) fn html(literal: &str, context: &Context) -> Vec<Line> {
    let muted = Style::fg(context.palette().muted);
    literal.trim_end_matches('\n').lines().flat_map(|line| text::wrap(&[Span::new(line, muted)], context.width(), &[])).collect()
}

/// A container too deep or too narrow to nest: its bare text, wrapped at the current width.
pub(super) fn flat<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Block> {
    layout::lines(text::wrap(&[Span::new(inline::text(node), context.text)], context.width(), &[]))
}

/// A fenced or indented code block: a Mermaid diagram or a code panel, both at the current width.
pub(super) fn code(block: &NodeCodeBlock, context: &Context) -> Vec<Block> {
    let language = block.info.split_whitespace().next();
    match language {
        Some(MERMAID) => vec![crate::mermaid::render(&block.literal, &context.settings)],
        _ => layout::lines(crate::code::render(&block.literal, language, &context.settings)),
    }
}
