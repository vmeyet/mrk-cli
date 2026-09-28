use comrak::nodes::{AstNode, NodeValue};

use super::context::Context;
use super::footnote;
use crate::document::{Span, Style};

/// Inline nesting deeper than this renders as its bare text: bounded recursion on hostile input.
const MAX_INLINE_NESTING: usize = 64;
const IMAGE_MARKER: &str = "▣ ";
const IMAGE_FALLBACK_ALT: &str = "image";

/// The styled spans of a node's inline children, each layered on `base`.
pub(super) fn spans<'a>(node: &'a AstNode<'a>, base: Style, context: &Context) -> Vec<Span> {
    children(node, base, context, 0)
}

/// The bare text under a node, blocks a space apart, walked without recursion.
pub(super) fn text<'a>(node: &'a AstNode<'a>) -> String {
    node.descendants()
        .map(|descendant| match &descendant.data().value {
            NodeValue::Text(text) => text.to_string(),
            NodeValue::Code(code) => code.literal.clone(),
            NodeValue::HtmlInline(html) => html.clone(),
            NodeValue::CodeBlock(block) => format!(" {}", block.literal),
            NodeValue::SoftBreak | NodeValue::LineBreak | NodeValue::Paragraph | NodeValue::Heading(_) | NodeValue::TableCell => {
                " ".to_owned()
            }
            _ => String::new(),
        })
        .collect()
}

fn children<'a>(node: &'a AstNode<'a>, base: Style, context: &Context, depth: usize) -> Vec<Span> {
    if depth >= MAX_INLINE_NESTING {
        return vec![Span::new(text(node), base)];
    }
    node.children().flat_map(|child| inline(child, base, context, depth + 1)).collect()
}

fn inline<'a>(node: &'a AstNode<'a>, base: Style, context: &Context, depth: usize) -> Vec<Span> {
    let palette = context.palette();
    match &node.data().value {
        NodeValue::Text(text) => vec![Span::new(text.to_string(), base)],
        NodeValue::SoftBreak => vec![Span::new(" ", base)],
        NodeValue::LineBreak => vec![Span::new("\n", base)],
        NodeValue::Code(code) => {
            vec![Span::new(format!(" {} ", code.literal), Style { fg: Some(palette.code), bg: Some(palette.surface), ..base })]
        }
        NodeValue::HtmlInline(html) => vec![Span::new(html.replace('\n', " "), Style { fg: Some(palette.muted), ..base })],
        NodeValue::Emph => children(node, base.italic(), context, depth),
        NodeValue::Strong => children(node, base.bold(), context, depth),
        NodeValue::Strikethrough => children(node, base.strike().dim(), context, depth),
        NodeValue::Link(link) => linked(children(node, Style { fg: Some(palette.link), ..base }.underline(), context, depth), &link.url),
        NodeValue::Image(image) => {
            linked(vec![Span::new(format!("{IMAGE_MARKER}{}", alt(node)), Style { fg: Some(palette.muted), ..base })], &image.url)
        }
        NodeValue::FootnoteReference(reference) => {
            vec![Span::new(footnote::marker(reference.ix), Style { fg: Some(palette.accent), ..base })]
        }
        _ => children(node, base, context, depth),
    }
}

fn alt<'a>(image: &'a AstNode<'a>) -> String {
    let alt = text(image);
    if alt.trim().is_empty() { IMAGE_FALLBACK_ALT.to_owned() } else { alt }
}

fn linked(spans: Vec<Span>, url: &str) -> Vec<Span> {
    if url.is_empty() {
        return spans;
    }
    spans.into_iter().map(|span| span.linked(url)).collect()
}

#[cfg(test)]
mod tests {
    use crate::markdown::{plain_at, span_with};
    use crate::theme::MRK_DARK;

    #[test]
    fn link_is_underlined_in_link_colour_with_its_target() {
        let link = span_with("see [the docs](https://example.com/docs)", "docs");

        assert_eq!(link.link.as_deref(), Some("https://example.com/docs"));
        assert_eq!((link.style.fg, link.style.underline), (Some(MRK_DARK.palette.link), true));
    }

    #[test]
    fn autolink_is_a_link_to_itself() {
        assert_eq!(span_with("visit https://example.org now", "example").link.as_deref(), Some("https://example.org"));
    }

    #[test]
    fn empty_link_target_links_nothing() {
        assert_eq!(span_with("[text]()", "text").link, None);
    }

    #[test]
    fn strike_is_struck_and_dimmed() {
        let struck = span_with("a ~~gone~~ b", "gone");

        assert!(struck.style.strike && struck.style.dim);
    }

    #[test]
    fn emphasis_layers_bold_and_italic() {
        let both = span_with("***both***", "both");

        assert!(both.style.bold && both.style.italic);
    }

    #[test]
    fn inline_code_is_padded_on_surface() {
        let code = span_with("run `mrk` now", "mrk");

        assert_eq!(code.text, " mrk ");
        assert_eq!((code.style.fg, code.style.bg), (Some(MRK_DARK.palette.code), Some(MRK_DARK.palette.surface)));
    }

    #[test]
    fn image_is_muted_alt_text_linked_to_its_source() {
        let image = span_with("![a cat](cat.png)", "cat");

        assert_eq!(image.text, "▣ a cat");
        assert_eq!(image.link.as_deref(), Some("cat.png"));
        assert_eq!(image.style.fg, Some(MRK_DARK.palette.muted));
    }

    #[test]
    fn image_without_alt_still_shows() {
        assert_eq!(plain_at("![](cat.png)", 80), "▣ image\n");
    }

    #[test]
    fn footnote_reference_is_an_accent_superscript() {
        let reference = span_with("word[^a]\n\n[^a]: note", "¹");

        assert_eq!(reference.style.fg, Some(MRK_DARK.palette.accent));
    }

    #[test]
    fn no_markup_is_left_on_screen() {
        assert_eq!(plain_at("**bold** *italic* ~~struck~~", 80), "bold italic struck\n");
    }
}
