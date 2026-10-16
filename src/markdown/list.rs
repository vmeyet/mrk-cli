use comrak::nodes::{AstNode, ListDelimType, ListType, NodeList, NodeValue};

use super::context::{Context, Nested};
use super::layout::{self, Spacing};
use crate::document::{Block, Line, Span, Style};
use crate::text;

const BULLETS: [&str; 3] = ["•", "◦", "▪"];
const DONE: &str = "✔";
const OPEN: &str = "○";

/// What stands before an item: its glyph, its number, or its number then its task glyph, and whether the item is a
/// finished task.
struct Marker {
    spans: Vec<Span>,
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
    let last_number = list.start + items.len().saturating_sub(1);
    let number_width = last_number.to_string().len();
    let markers: Vec<Marker> =
        items.iter().zip(list.start..).map(|(item, number)| marker(item, list, number, number_width, context)).collect();
    let marker_width = markers.iter().map(|marker| width(&marker.spans)).max().unwrap_or(0) + 1;
    items.iter().zip(&markers).map(|(item, marker)| render_item(item, marker, marker_width, spacing(list), context)).collect()
}

fn marker<'a>(item: &'a AstNode<'a>, list: &NodeList, number: usize, number_width: usize, context: &Context) -> Marker {
    let palette = context.palette();
    let accent = Style::fg(palette.accent);
    let done = match &item.data().value {
        NodeValue::TaskItem(task) => Some(task.symbol.is_some()),
        _ => None,
    };
    let task = done.map(|done| if done { Span::new(DONE, Style::fg(palette.success)) } else { Span::new(OPEN, Style::fg(palette.muted)) });
    let lead = match list.list_type {
        ListType::Ordered => Some(Span::new(format!("{number:>number_width$}{}", delimiter(list.delimiter)), accent)),
        ListType::Bullet if task.is_none() => Some(Span::new(BULLETS[context.list_depth % BULLETS.len()], accent)),
        ListType::Bullet => None,
    };
    let spans = match (lead, task) {
        (Some(lead), Some(task)) => vec![lead, Span::plain(" "), task],
        (lead, task) => lead.into_iter().chain(task).collect(),
    };
    Marker { spans, is_done: done == Some(true) }
}

fn delimiter(delimiter: ListDelimType) -> char {
    match delimiter {
        ListDelimType::Period => '.',
        ListDelimType::Paren => ')',
    }
}

fn width(spans: &[Span]) -> usize {
    spans.iter().map(|span| text::display_width(&span.text)).sum()
}

fn render_item<'a>(item: &'a AstNode<'a>, marker: &Marker, marker_width: usize, spacing: Spacing, context: &Context) -> Vec<Block> {
    let padding = Span::plain(" ".repeat(marker_width - width(&marker.spans)));
    let lead: Vec<Span> = marker.spans.iter().cloned().chain([padding]).collect();
    let dimmed = |body: Vec<Block>| if marker.is_done { layout::restyle(body, Style::dim) } else { body };
    match context.nested(marker_width) {
        Nested::Indented(inner) => {
            let body = dimmed(super::children(item, &inner.in_list(), spacing));
            let body = if body.is_empty() { layout::lines(vec![Line::blank()]) } else { body };
            layout::prefix(body, &lead, &layout::blank(marker_width))
        }
        Nested::Unindented(inner) => dimmed(super::unindented(item, &lead, &inner.in_list(), spacing)),
        Nested::TooDeep => super::raw::flat(item, context),
    }
}

#[cfg(test)]
mod tests {
    use crate::markdown::{plain_at, plain_with_pictures, span_with};
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
    fn an_ordered_task_shows_its_number_then_its_state() {
        assert_eq!(plain_at("1. [x] one\n2. [ ] two\n3. three", 40), "1. ✔ one\n2. ○ two\n3.   three\n");
    }

    #[test]
    fn items_with_no_room_to_indent_keep_their_markers_at_the_last_indent() {
        let source = "- a\n  - b\n    - c\n      - d\n        - e\n          - f\n            - g\n\n              ```\n              code\n              ```";

        insta::assert_snapshot!(plain_at(source, 20));
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
    fn an_item_opening_on_a_diagram_has_its_marker_above_the_indented_picture() {
        let plain = plain_with_pictures("- ```mermaid\n  graph LR\n    A --> B\n  ```\n- after");

        assert!(plain.starts_with("•\n  [picture: flowchart "), "{plain}");
        assert!(plain.ends_with("]\n• after\n"), "{plain}");
    }

    #[test]
    fn empty_item_keeps_its_marker() {
        assert_eq!(plain_at("- \n- b", 40), "•\n• b\n");
    }
}
