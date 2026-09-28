use comrak::nodes::{AstNode, ListDelimType, ListType, NodeList, NodeValue};

use super::context::Context;
use super::layout::{self, Spacing};
use crate::document::{Block, Line, Span, Style};
use crate::text;

const BULLETS: [&str; 3] = ["•", "◦", "▪"];
const DONE: &str = "✔";
const OPEN: &str = "○";

/// What stands before an item: its glyph or number, and whether the item is a finished task.
struct Marker {
    text: String,
    style: Style,
    is_done: bool,
}

pub(super) fn render<'a>(node: &'a AstNode<'a>, list: &NodeList, context: &Context) -> Vec<Block> {
    layout::stack(items(node, list, context), spacing(list))
}

/// Tight items sit on consecutive lines, loose ones a blank line apart.
pub(super) fn spacing(list: &NodeList) -> Spacing {
    if list.tight { Spacing::Tight } else { Spacing::Loose }
}

/// Each item rendered on its own, marker included, in the order of the list.
pub(super) fn items<'a>(node: &'a AstNode<'a>, list: &NodeList, context: &Context) -> Vec<Vec<Block>> {
    let items: Vec<&AstNode> = node.children().collect();
    let numbers = (list.start..).take(items.len());
    let number_width = numbers.clone().last().map_or(1, |last| last.to_string().len());
    let markers: Vec<Marker> = items.iter().zip(numbers).map(|(item, number)| marker(item, list, number, number_width, context)).collect();
    let marker_width = markers.iter().map(|marker| text::display_width(&marker.text)).max().unwrap_or(0) + 1;
    items.iter().zip(&markers).map(|(item, marker)| render_item(item, marker, marker_width, spacing(list), context)).collect()
}

fn marker<'a>(item: &'a AstNode<'a>, list: &NodeList, number: usize, number_width: usize, context: &Context) -> Marker {
    let palette = context.palette();
    let accent = Style::fg(palette.accent);
    match (&item.data().value, list.list_type) {
        (NodeValue::TaskItem(task), _) if task.symbol.is_some() => {
            Marker { text: DONE.to_owned(), style: Style::fg(palette.success), is_done: true }
        }
        (NodeValue::TaskItem(_), _) => Marker { text: OPEN.to_owned(), style: Style::fg(palette.muted), is_done: false },
        (_, ListType::Ordered) => {
            Marker { text: format!("{number:>number_width$}{}", delimiter(list.delimiter)), style: accent, is_done: false }
        }
        (_, ListType::Bullet) => Marker { text: BULLETS[context.list_depth % BULLETS.len()].to_owned(), style: accent, is_done: false },
    }
}

fn delimiter(delimiter: ListDelimType) -> char {
    match delimiter {
        ListDelimType::Period => '.',
        ListDelimType::Paren => ')',
    }
}

fn render_item<'a>(item: &'a AstNode<'a>, marker: &Marker, marker_width: usize, spacing: Spacing, context: &Context) -> Vec<Block> {
    let Some(inner) = context.nested(marker_width) else { return super::raw::flat(item, context) };
    let body = super::children(item, &inner.in_list(), spacing);
    let body = if marker.is_done { layout::restyle(body, Style::dim) } else { body };
    let body = if body.is_empty() { layout::lines(vec![Line::blank()]) } else { body };
    let first = vec![Span::new(format!("{:<marker_width$}", marker.text), marker.style)];
    layout::prefix(body, &first, &layout::blank(marker_width))
}

#[cfg(test)]
mod tests {
    use crate::markdown::{plain_at, span_with};
    use crate::theme::MRK_DARK;

    #[test]
    fn bullets_change_glyph_with_depth() {
        insta::assert_snapshot!(plain_at("- one\n  - two\n    - three\n      - four", 40));
    }

    #[test]
    fn numbers_are_right_aligned_from_the_start_number() {
        insta::assert_snapshot!(plain_at("9. nine\n10. ten\n11. eleven with text that wraps under itself", 24));
    }

    #[test]
    fn loose_items_are_a_blank_line_apart() {
        insta::assert_snapshot!(plain_at("- one\n\n  second paragraph\n\n- two", 40));
    }

    #[test]
    fn mixed_nesting_of_lists_and_quotes() {
        insta::assert_snapshot!(plain_at("1. first\n   - bullet\n     > quoted\n2) second", 40));
    }

    #[test]
    fn tasks_show_their_state() {
        insta::assert_snapshot!(plain_at("- [x] done\n- [ ] open\n- plain", 40));
    }

    #[test]
    fn done_task_is_dimmed_behind_a_success_check() {
        let palette = MRK_DARK.palette;

        assert_eq!(span_with("- [x] done", "✔").style.fg, Some(palette.success));
        assert!(span_with("- [x] done", "done").style.dim);
        assert_eq!(span_with("- [ ] open", "○").style.fg, Some(palette.muted));
        assert!(!span_with("- [ ] open", "open").style.dim);
    }

    #[test]
    fn markers_are_accent() {
        assert_eq!(span_with("- a", "•").style.fg, Some(MRK_DARK.palette.accent));
        assert_eq!(span_with("3. a", "3.").style.fg, Some(MRK_DARK.palette.accent));
    }

    #[test]
    fn empty_item_keeps_its_marker() {
        assert_eq!(plain_at("- \n- b", 40), "•\n• b\n");
    }
}
