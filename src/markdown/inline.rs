use std::borrow::Cow;
use std::sync::Arc;

use comrak::nodes::{AstNode, NodeValue};

use super::context::Context;
use super::footnote;
use crate::document::{Span, Style};
use crate::sanitize;
use crate::theme::Palette;

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
        NodeValue::Code(code) => vec![Span::new(format!(" {} ", code.literal), inline_code(base, palette))],
        NodeValue::HtmlInline(html) => vec![Span::new(html.replace('\n', " "), base.with_fg(palette.muted))],
        NodeValue::Emph => children(node, base.italic(), context, depth),
        NodeValue::Strong => children(node, base.bold(), context, depth),
        NodeValue::Strikethrough => children(node, base.strike().dim(), context, depth),
        NodeValue::Link(link) => {
            let text = children(node, base.with_fg(palette.link).underline(), context, depth);
            linked(text, &link.url, Target::Link, base, context)
        }
        NodeValue::Image(image) => {
            let text = vec![Span::new(format!("{IMAGE_MARKER}{}", alt(node)), base.with_fg(palette.muted))];
            linked(text, &image.url, Target::Image, base, context)
        }
        NodeValue::FootnoteReference(reference) => {
            vec![Span::new(footnote::marker(reference.ix), base.with_fg(palette.accent))]
        }
        _ => children(node, base, context, depth),
    }
}

/// Muted toward `text` so a sentence full of identifiers still reads as prose.
fn inline_code(base: Style, palette: &Palette) -> Style {
    base.with_fg(palette.text.mix(palette.code, 0.20)).on(palette.surface.mix(palette.code, 0.05))
}

fn alt<'a>(image: &'a AstNode<'a>) -> String {
    let alt = text(image);
    if alt.trim().is_empty() { IMAGE_FALLBACK_ALT.to_owned() } else { alt }
}

/// What a URL points at: what is worth printing after the text when it cannot be clicked differs.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Link,
    Image,
}

fn linked(spans: Vec<Span>, url: &str, target: Target, base: Style, context: &Context) -> Vec<Span> {
    if url.is_empty() {
        return spans;
    }
    let resolved = context.settings.link_base.as_ref().map_or(Cow::Borrowed(url), |link_base| link_base.resolve(url));
    let clickable: Option<Arc<str>> = sanitize::link(&resolved).map(Arc::from);
    let shown = if url.starts_with('#') { None } else { shown_target(&spans, &resolved, target, clickable.is_some(), base, context) };
    let spans = spans.into_iter().map(|span| match &clickable {
        Some(clickable) => span.linked(Arc::clone(clickable)),
        None => span,
    });
    spans.chain(shown).collect()
}

/// ` <url>` for a link that cannot be clicked, unless its text already reads as the URL. Without hyperlinks every
/// target shows; with them, only a link's target that `sanitize::link` refuses (a relative path, another scheme), and
/// never an image's source. The caller leaves out a written `#fragment`, which points inside the document.
fn shown_target(spans: &[Span], url: &str, target: Target, is_clickable: bool, base: Style, context: &Context) -> Option<Span> {
    let is_unclickable = !context.settings.hyperlinks || (target == Target::Link && !is_clickable);
    if !is_unclickable {
        return None;
    }
    let text: String = spans.iter().map(|span| span.text.as_str()).collect();
    (!reads_as(text.trim(), url)).then(|| Span::new(format!(" <{url}>"), base.with_fg(context.palette().muted)))
}

/// Whether `text` is `url`, or the address an autolink made it from: comrak adds `mailto:` to an email and `http://`
/// to a bare `www.` address.
fn reads_as(text: &str, url: &str) -> bool {
    let is_www = text.starts_with("www.") && url.strip_prefix("http://") == Some(text);
    text == url || url.strip_prefix("mailto:") == Some(text) || is_www
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use std::sync::Arc;

    use crate::document::{Block, Document, Settings, Span, Style, plain, test_settings};
    use crate::link::LinkBase;
    use crate::markdown::{plain_at, render, styled};
    use crate::theme::MRK_DARK;

    fn without_hyperlinks(source: &str, width: usize) -> String {
        plain(&render(source, &Settings { width, hyperlinks: false, ..test_settings() }))
    }

    fn text() -> Style {
        Style::fg(MRK_DARK.palette.text)
    }

    fn link_style() -> Style {
        text().with_fg(MRK_DARK.palette.link).underline()
    }

    #[test]
    fn link_is_underlined_in_link_colour_with_its_target() {
        assert_eq!(
            styled("see [the docs](https://example.com/docs)"),
            [[Span::new("see ", text()), Span::new("the docs", link_style()).linked("https://example.com/docs")]]
        );
    }

    #[test]
    fn autolink_is_a_link_to_itself() {
        assert_eq!(
            styled("visit https://example.org now"),
            [[
                Span::new("visit ", text()),
                Span::new("https://example.org", link_style()).linked("https://example.org"),
                Span::new(" now", text())
            ]]
        );
    }

    #[test]
    fn empty_link_target_links_nothing() {
        assert_eq!(styled("[text]()"), [[Span::new("text", link_style())]]);
    }

    #[test]
    fn a_clickable_link_shows_only_its_text() {
        assert_eq!(plain_at("see [the docs](https://example.com/docs)", 80), "see the docs\n");
    }

    #[test]
    fn without_hyperlinks_a_link_shows_its_target_after_the_text() {
        assert_eq!(without_hyperlinks("see [the docs](https://example.com/docs)", 80), "see the docs <https://example.com/docs>\n");
    }

    #[test]
    fn without_hyperlinks_an_image_shows_its_source_after_the_alt_text() {
        assert_eq!(without_hyperlinks("![a cat](cat.png)", 80), "▣ a cat <cat.png>\n");
    }

    #[test]
    fn without_hyperlinks_autolinks_are_not_repeated() {
        assert_eq!(
            without_hyperlinks("visit https://example.org, www.example.com or <me@example.org>", 80),
            "visit https://example.org, www.example.com or me@example.org\n"
        );
    }

    #[test]
    fn without_hyperlinks_a_link_written_by_hand_shows_its_target_even_when_it_looks_like_its_text() {
        assert_eq!(without_hyperlinks("[example.com](http://example.com)", 80), "example.com <http://example.com>\n");
        assert_eq!(without_hyperlinks("[x](http://x)", 80), "x <http://x>\n");
    }

    #[test]
    fn without_hyperlinks_the_target_wraps_with_the_text() {
        assert_eq!(without_hyperlinks("see [the docs](https://example.com/docs)", 30), "see the docs\n<https://example.com/docs>\n");
    }

    #[test]
    fn strike_is_struck_and_dimmed() {
        assert_eq!(styled("a ~~gone~~ b"), [[Span::new("a ", text()), Span::new("gone", text().strike().dim()), Span::new(" b", text())]]);
    }

    #[test]
    fn emphasis_layers_bold_and_italic() {
        assert_eq!(styled("***both***"), [[Span::new("both", text().bold().italic())]]);
    }

    #[test]
    fn inline_code_is_padded_and_tinted_toward_the_code_colour() {
        let palette = MRK_DARK.palette;
        let code = Style::fg(palette.text.mix(palette.code, 0.20)).on(palette.surface.mix(palette.code, 0.05));

        assert_eq!(styled("run `mrk` now"), [[Span::new("run ", text()), Span::new(" mrk ", code), Span::new(" now", text())]]);
    }

    #[test]
    fn image_is_muted_alt_text_linked_to_its_source() {
        let muted = Style::fg(MRK_DARK.palette.muted);

        assert_eq!(styled("![a cat](https://x.y/cat.png)"), [[Span::new("▣ a cat", muted).linked("https://x.y/cat.png")]]);
    }

    fn links_at(source: &str, width: usize) -> Vec<Option<Arc<str>>> {
        links(&render(source, &Settings { width, ..test_settings() }))
    }

    fn links(document: &Document) -> Vec<Option<Arc<str>>> {
        let lines = document.blocks.iter().flat_map(Block::lines);
        lines.flat_map(|line| &line.spans).map(|span| span.link.clone()).collect()
    }

    #[test]
    fn a_link_no_terminal_would_open_links_nothing() {
        let too_long = format!("https://x.y/{}", "a".repeat(3000));

        for source in [too_long.as_str(), "[guide](docs/guide.md)", "[x](javascript:alert(1))"] {
            assert!(links_at(source, 80).iter().all(Option::is_none), "{source}");
        }
    }

    fn with_base(source: &str, hyperlinks: bool) -> Document {
        let link_base = Some(LinkBase::new("https://forge.test/group/project/-/merge_requests/7").unwrap());
        render(source, &Settings { hyperlinks, link_base, ..test_settings() })
    }

    fn links_with_base(source: &str) -> Vec<Option<Arc<str>>> {
        links(&with_base(source, true))
    }

    #[test]
    fn with_a_link_base_a_relative_link_or_image_is_clickable() {
        assert_eq!(
            links_with_base("[report](/uploads/ab12/report.pdf) ![a](../../raw/main/a.png)"),
            [
                Some(Arc::from("https://forge.test/uploads/ab12/report.pdf")),
                None,
                Some(Arc::from("https://forge.test/group/project/raw/main/a.png"))
            ]
        );
        assert_eq!(plain(&with_base("[report](/uploads/ab12/report.pdf)", true)), "report\n");
    }

    #[test]
    fn with_a_rooted_link_base_a_gitlab_upload_links_under_the_project() {
        let link_base = LinkBase::new("https://gitlab.example.com/group/project/-/merge_requests/7")
            .and_then(|base| base.rooted_at("/uploads/", "https://gitlab.example.com/group/project"))
            .ok();
        let document = render("[x](/uploads/ab/x.pdf) [y](/group/other)", &Settings { link_base, ..test_settings() });

        assert_eq!(
            links(&document),
            [
                Some(Arc::from("https://gitlab.example.com/group/project/uploads/ab/x.pdf")),
                None,
                Some(Arc::from("https://gitlab.example.com/group/other"))
            ]
        );
    }

    #[test]
    fn with_a_link_base_and_no_hyperlinks_the_resolved_target_shows() {
        assert_eq!(
            plain(&with_base("[x](docs/x.md) and [notes](#note_2)", false)),
            "x <https://forge.test/group/project/-/merge_requests/docs/x.md> and notes\n"
        );
    }

    #[test]
    fn a_link_base_never_makes_a_refused_scheme_clickable() {
        for source in ["[x](javascript:alert(1))", "[x](data:text/html,x)", "[x](file://host/share)"] {
            assert!(links_with_base(source).iter().all(Option::is_none), "{source}");
        }
    }

    #[test]
    fn a_resolved_target_is_still_sanitized() {
        let links = links_with_base("[x](<a\u{1b}]52;c;eA==\u{7}b\u{9b}c>)");

        assert_eq!(links, [Some(Arc::from("https://forge.test/group/project/-/merge_requests/a]52;c;eA==bc"))]);
    }

    #[test]
    fn without_a_link_base_a_relative_link_stays_unclickable() {
        assert_eq!(links_at("[report](/uploads/ab12/report.pdf)", 80), [None, None]);
        assert_eq!(plain_at("[report](/uploads/ab12/report.pdf)", 80), "report </uploads/ab12/report.pdf>\n");
    }

    #[test]
    fn a_wrapped_link_shares_one_target() {
        let links = links_at(&format!("https://x.y/{}", "a".repeat(200)), 40);
        let targets: Vec<&Arc<str>> = links.iter().flatten().collect();

        assert_eq!((links.len(), targets.len()), (6, 6));
        assert!(targets.iter().all(|target| Arc::ptr_eq(target, targets[0])));
    }

    #[test]
    fn image_without_alt_still_shows() {
        assert_eq!(plain_at("![](https://x.y/cat.png)", 80), "▣ image\n");
    }

    #[test]
    fn a_link_no_terminal_can_click_shows_its_target_even_with_hyperlinks() {
        assert_eq!(plain_at("see [the guide](docs/guide.md) and ![logo](a.png)", 80), "see the guide <docs/guide.md> and ▣ logo\n");
    }

    #[test]
    fn a_fragment_or_a_target_its_code_text_reads_as_is_never_shown() {
        for hyperlinks in [true, false] {
            let plain = plain(&render("[Install](#install) and [`AGENTS.md`](AGENTS.md)", &Settings { hyperlinks, ..test_settings() }));

            assert_eq!(plain, "Install and  AGENTS.md\n", "hyperlinks {hyperlinks}");
        }
    }

    #[test]
    fn footnote_reference_is_an_accent_superscript() {
        let lines = styled("word[^a]\n\n[^a]: note");

        assert_eq!(lines[0], [Span::new("word", text()), Span::new("¹", Style::fg(MRK_DARK.palette.accent))]);
    }

    #[cfg(feature = "cli")]
    #[test]
    fn inline_styles_in_colour() {
        let source = "Run `mrk` on **bold**, *italic* and ~~gone~~ text, see [the docs](https://example.com) or www.example.org.";

        insta::assert_snapshot!(crate::markdown::ansi_at(source, 60));
    }

    #[test]
    fn no_markup_is_left_on_screen() {
        assert_eq!(plain_at("**bold** *italic* ~~struck~~", 80), "bold italic struck\n");
    }
}
