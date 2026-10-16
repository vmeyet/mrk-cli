use comrak::nodes::{AstNode, NodeCodeBlock};

use super::context::Context;
use super::inline;
use super::layout;
use crate::document::{Block, Line, Span, Style};
use crate::text;

pub(super) const MERMAID: &str = "mermaid";

/// An HTML block as muted literal text, never interpreted: cut where a line is full, so its spacing stays.
pub(super) fn html(literal: &str, context: &Context) -> Vec<Line> {
    let muted = Style::fg(context.palette().muted);
    literal.trim_end_matches('\n').lines().flat_map(|line| text::hard_wrap(&[Span::new(line, muted)], context.width(), &[])).collect()
}

/// A container nested too deep: its bare text, wrapped at the current width.
pub(super) fn flat<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Block> {
    super::wrapped(&[Span::new(inline::text(node), context.style)], context)
}

/// A fenced or indented code block: a Mermaid diagram or a code panel, both at the current width.
pub(super) fn code(block: &NodeCodeBlock, context: &Context) -> Vec<Block> {
    let language = language(block);
    match language.as_deref() {
        Some(MERMAID) => vec![crate::mermaid::render(&block.literal, &context.settings)],
        language => layout::lines(crate::code::render(&block.literal, language, &context.settings)),
    }
}

/// The language a fence names, lowercase: the first word of its info string (`rust,ignore`, `{.python}` and
/// `ts title="a.ts"` all name one language).
pub(super) fn language(block: &NodeCodeBlock) -> Option<String> {
    let words = block.info.split(|character: char| character.is_whitespace() || character == ',');
    words.map(|word| word.trim_matches(['{', '}', '.'])).find(|word| !word.is_empty()).map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use comrak::nodes::NodeCodeBlock;

    use crate::markdown::plain_at;

    fn language(info: &str) -> Option<String> {
        super::language(&NodeCodeBlock { info: info.to_owned(), ..NodeCodeBlock::default() })
    }

    #[test]
    fn the_language_is_the_first_word_of_the_info_string_lowercased() {
        assert_eq!(language("rust,ignore").as_deref(), Some("rust"));
        assert_eq!(language("  ts title=\"a.ts\"").as_deref(), Some("ts"));
        assert_eq!(language("{.python}").as_deref(), Some("python"));
        assert_eq!(language("Mermaid").as_deref(), Some("mermaid"));
        assert_eq!(language("   "), None);
    }

    #[test]
    fn an_html_block_keeps_its_spacing_and_cuts_where_a_line_is_full() {
        assert_eq!(plain_at("<pre>a    b</pre>\n", 40), "<pre>a    b</pre>\n");
        assert_eq!(plain_at("<pre>a    bcdefgh</pre>\n", 10), "<pre>a\nbcdefgh</p\nre>\n");
    }

    #[test]
    fn the_code_panel_is_labelled_with_the_language_only() {
        assert_eq!(plain_at("```Rust,ignore\nx\n```", 20).lines().next().map(str::trim), Some("rust"));
    }
}
