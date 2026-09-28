use comrak::nodes::{AlertType, AstNode, NodeAlert};

use super::context::Context;
use super::layout::{self, Spacing};
use crate::document::{Block, Rgb, Span, Style};
use crate::text;

const BAR: &str = "│ ";

/// A block quote: a `subtle` bar before every line, the text italic `muted`.
pub(super) fn render<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Block> {
    let palette = context.palette();
    let Some(inner) = context.nested(text::display_width(BAR)) else { return super::raw::flat(node, context) };
    let inner = inner.with_text(Style::fg(palette.muted).italic());
    let body = super::children(node, &inner, Spacing::Loose);
    barred(body, palette.subtle)
}

/// A GitHub alert: the bar and the `● Note` title in the alert colour, the body in `text`.
pub(super) fn alert<'a>(node: &'a AstNode<'a>, alert: &NodeAlert, context: &Context) -> Vec<Block> {
    let palette = context.palette();
    let (glyph, color) = match alert.alert_type {
        AlertType::Note => ("●", palette.note),
        AlertType::Tip => ("◆", palette.tip),
        AlertType::Important => ("✦", palette.important),
        AlertType::Warning => ("▲", palette.warning),
        AlertType::Caution => ("■", palette.caution),
    };
    let Some(inner) = context.nested(text::display_width(BAR)) else { return super::raw::flat(node, context) };
    let inner = inner.with_text(Style::fg(palette.text));
    let title_text = alert.title.clone().unwrap_or_else(|| alert.alert_type.default_title().to_owned());
    let title = text::wrap(&[Span::new(format!("{glyph} {title_text}"), Style::fg(color).bold())], inner.width(), &[Span::plain("  ")]);
    let body = super::children(node, &inner, Spacing::Loose);
    barred(layout::stack([layout::lines(title), body], Spacing::Tight), color)
}

fn barred(body: Vec<Block>, color: Rgb) -> Vec<Block> {
    let bar = [Span::new(BAR, Style::fg(color))];
    layout::prefix(body, &bar, &bar)
}

#[cfg(test)]
mod tests {
    use crate::markdown::{plain_at, span_with};
    use crate::theme::MRK_DARK;

    #[test]
    fn nested_quotes_stack_their_bars() {
        insta::assert_snapshot!(plain_at("> outer\n>\n> > inner that wraps onto a second line\n>\n> back out", 30));
    }

    #[test]
    fn quote_text_is_italic_muted_behind_a_subtle_bar() {
        let palette = MRK_DARK.palette;
        let text = span_with("> quoted", "quoted");

        assert_eq!((text.style.fg, text.style.italic), (Some(palette.muted), true));
        assert_eq!(span_with("> quoted", "│").style.fg, Some(palette.subtle));
    }

    #[test]
    fn alerts_carry_their_glyph_and_title() {
        let source = "> [!NOTE]\n> n\n\n> [!TIP]\n> t\n\n> [!IMPORTANT]\n> i\n\n> [!WARNING]\n> w\n\n> [!CAUTION]\n> c";

        insta::assert_snapshot!(plain_at(source, 40));
    }

    #[test]
    fn alert_bar_and_title_take_the_alert_colour_and_the_body_stays_text() {
        let palette = MRK_DARK.palette;
        let source = "> [!WARNING]\n> Careful here.";

        assert_eq!(span_with(source, "Warning").style.fg, Some(palette.warning));
        assert_eq!(span_with(source, "│").style.fg, Some(palette.warning));
        assert_eq!(span_with(source, "Careful").style, crate::document::Style::fg(palette.text));
    }
}
