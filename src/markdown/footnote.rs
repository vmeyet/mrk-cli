use comrak::nodes::AstNode;

use super::context::Context;
use super::layout::{self, Spacing};
use super::rule;
use crate::document::{Block, Line, Span, Style};

const SUPERSCRIPT_DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
const RULE_WIDTH: usize = 16;

/// The footnote number as superscript digits: `12` is `¹²`.
pub(super) fn marker(number: u32) -> String {
    number.to_string().chars().filter_map(|digit| digit.to_digit(10)).map(|digit| SUPERSCRIPT_DIGITS[digit as usize]).collect()
}

/// The notes listed at the end under a short rule, numbered in the order comrak sorted them (first reference first).
pub(super) fn section<'a>(definitions: &[&'a AstNode<'a>], context: &Context) -> Vec<Block> {
    if definitions.is_empty() {
        return Vec::new();
    }
    let short_rule = layout::lines(vec![rule::short(RULE_WIDTH, context)]);
    let markers: Vec<String> = (1..=definitions.len()).map(|number| marker(number as u32)).collect();
    let marker_width = markers.iter().map(|marker| crate::text::display_width(marker)).max().unwrap_or(0) + 1;
    let notes = definitions.iter().zip(markers).map(|(definition, marker)| note(definition, &marker, marker_width, context));
    layout::stack(std::iter::once(short_rule).chain(notes), Spacing::Tight)
}

fn note<'a>(definition: &'a AstNode<'a>, marker: &str, marker_width: usize, context: &Context) -> Vec<Block> {
    let Some(inner) = context.nested(marker_width) else { return super::raw::flat(definition, context) };
    let body = super::children(definition, &inner, Spacing::Loose);
    let padded = format!("{marker:<marker_width$}");
    let first = vec![Span::new(padded, Style::fg(context.palette().accent))];
    let body = if body.is_empty() { layout::lines(vec![Line::blank()]) } else { body };
    layout::prefix(body, &first, &layout::blank(marker_width))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_is_superscript_digits() {
        assert_eq!(marker(1), "¹");
        assert_eq!(marker(42), "⁴²");
    }

    #[test]
    fn notes_are_listed_at_the_end_in_reference_order() {
        let source = "b[^b] then a[^a].\n\n[^a]: Note a.\n[^b]: Note b, long enough to wrap.\n[^unused]: Never shown.\n\nLast paragraph.";

        insta::assert_snapshot!(crate::markdown::plain_at(source, 24));
    }
}
