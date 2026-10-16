use comrak::nodes::{AlertType, AstNode, NodeAlert};

use super::context::{Context, Nested};
use super::layout::{self, Spacing};
use crate::document::{Block, Rgb, Span, Style};
use crate::text;

const BAR: &str = "│ ";

/// A block quote: a `subtle` bar before every line, the text italic `muted`.
pub(super) fn render<'a>(node: &'a AstNode<'a>, context: &Context) -> Vec<Block> {
    let palette = context.palette();
    let quoted = Style::fg(palette.muted).italic();
    match context.nested(text::display_width(BAR)) {
        Nested::Indented(inner) => barred(super::children(node, &inner.with_style(quoted), Spacing::Loose), palette.subtle),
        Nested::Unindented(inner) => super::children(node, &inner.with_style(quoted), Spacing::Loose),
        Nested::TooDeep => super::raw::flat(node, context),
    }
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
    let titled = |inner: &Context| {
        let title_text = alert.title.as_deref().unwrap_or(alert.alert_type.default_title());
        let title = Span::new(format!("{glyph} {title_text}"), Style::fg(color).bold());
        let title = text::wrap(&[title], inner.width(), &[Span::plain("  ")]);
        let body = super::children(node, &inner.with_style(Style::fg(palette.text)), Spacing::Loose);
        layout::stack([layout::lines(title), body], Spacing::Tight)
    };
    match context.nested(text::display_width(BAR)) {
        Nested::Indented(inner) => barred(titled(&inner), color),
        Nested::Unindented(inner) => titled(&inner),
        Nested::TooDeep => super::raw::flat(node, context),
    }
}

fn barred(body: Vec<Block>, color: Rgb) -> Vec<Block> {
    let bar = [Span::new(BAR, Style::fg(color))];
    layout::prefix(body, &bar, &bar)
}

#[cfg(test)]
mod tests {
    use crate::document::{Span, Style};
    use crate::markdown::{plain_at, plain_with_pictures, styled};
    use crate::theme::MRK_DARK;

    #[test]
    fn nested_quotes_stack_their_bars() {
        insta::assert_snapshot!(plain_at("> outer\n>\n> > inner that wraps onto a second line\n>\n> back out", 30));
    }

    #[test]
    fn quotes_with_no_room_for_another_bar_stay_quotes_without_it() {
        assert_eq!(plain_at("> > > > one\n> > > >\n> > > > > two", 18), "│ │ │ one\n│ │ │\n│ │ │ two\n");
        assert_eq!(plain_at("> > > > > [!TIP]\n> > > > > tip", 18), "│ │ │ ◆ Tip\n│ │ │ tip\n");
    }

    #[test]
    fn quote_text_is_italic_muted_behind_a_subtle_bar() {
        let palette = MRK_DARK.palette;

        assert_eq!(
            styled("> quoted"),
            [[Span::new("│ ", Style::fg(palette.subtle)), Span::new("quoted", Style::fg(palette.muted).italic())]]
        );
    }

    #[test]
    fn a_diagram_in_a_quote_keeps_the_bar_beside_it() {
        let plain = plain_with_pictures("> ```mermaid\n> graph LR\n>   A --> B\n> ```");

        assert!(plain.starts_with("│ [picture: flowchart "), "{plain}");
    }

    #[test]
    fn alerts_carry_their_glyph_and_title() {
        let source = "> [!NOTE]\n> n\n\n> [!TIP]\n> t\n\n> [!IMPORTANT]\n> i\n\n> [!WARNING]\n> w\n\n> [!CAUTION]\n> c";

        insta::assert_snapshot!(plain_at(source, 40));
    }

    #[test]
    fn alert_bar_and_title_take_the_alert_colour_and_the_body_stays_text() {
        let palette = MRK_DARK.palette;
        let bar = Span::new("│ ", Style::fg(palette.warning));

        assert_eq!(
            styled("> [!WARNING]\n> Careful here."),
            [
                [bar.clone(), Span::new("▲ Warning", Style::fg(palette.warning).bold())],
                [bar, Span::new("Careful here.", Style::fg(palette.text))]
            ]
        );
    }
}
