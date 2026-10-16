use comrak::nodes::AstNode;

use super::context::{Context, Nested};
use super::layout::{self, Spacing};
use super::rule;
use crate::document::{Block, Span, Style};

const SUPERSCRIPT_DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
const RULE_WIDTH: usize = 16;

/// The footnote number as superscript digits: `12` is `¹²`.
pub(super) fn marker(number: u32) -> String {
    number.to_string().chars().filter_map(|digit| digit.to_digit(10)).map(|digit| SUPERSCRIPT_DIGITS[digit as usize]).collect()
}

/// The notes listed at the end under a short rule, numbered in the order comrak sorted them (first reference first).
pub(super) fn section<'a>(definitions: &[&'a AstNode<'a>], context: &Context) -> Vec<Block> {
    let short_rule = layout::lines(vec![rule::short(RULE_WIDTH, context)]);
    let markers: Vec<String> = (1..).take(definitions.len()).map(marker).collect();
    let marker_width = markers.iter().map(|marker| crate::text::display_width(marker)).max().unwrap_or(0) + 1;
    let notes = definitions.iter().zip(markers).map(|(definition, marker)| note(definition, &marker, marker_width, context));
    layout::stack(std::iter::once(short_rule).chain(notes), Spacing::Tight)
}

fn note<'a>(definition: &'a AstNode<'a>, marker: &str, marker_width: usize, context: &Context) -> Vec<Block> {
    let lead = [Span::new(format!("{marker:<marker_width$}"), Style::fg(context.palette().accent))];
    match context.nested(marker_width) {
        Nested::Indented(inner) => layout::marked(super::children(definition, &inner, Spacing::Loose), &lead, marker_width),
        Nested::Unindented(inner) => super::unindented(definition, &lead, &inner, Spacing::Loose),
        Nested::TooDeep => super::raw::flat(definition, context),
    }
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
    fn a_note_with_no_room_to_indent_keeps_its_marker() {
        let source = "a[^a]\n\n> > > > > [^a]: Deep note.";

        assert!(crate::markdown::plain_at(source, 12).ends_with("¹ Deep note.\n"));
    }

    #[test]
    fn notes_are_listed_at_the_end_in_reference_order() {
        let source = "b[^b] then a[^a].\n\n[^a]: Note a.\n[^b]: Note b, long enough to wrap.\n[^unused]: Never shown.\n\nLast paragraph.";

        insta::assert_snapshot!(crate::markdown::plain_at(source, 24));
    }
}
