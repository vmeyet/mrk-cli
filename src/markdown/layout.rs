use crate::document::{Block, Line, Span, Style};

/// Whether sibling blocks sit on consecutive lines (a tight list) or one blank line apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Spacing {
    Tight,
    Loose,
}

/// Joins groups of blocks, one blank line apart when loose, and fuses neighbouring line blocks into one.
pub(super) fn stack(groups: impl IntoIterator<Item = Vec<Block>>, spacing: Spacing) -> Vec<Block> {
    let separated = groups.into_iter().filter(|group| !is_empty(group)).enumerate().flat_map(|(index, group)| {
        let gap = (index > 0 && spacing == Spacing::Loose).then(|| Block::Lines(vec![Line::blank()]));
        gap.into_iter().chain(group)
    });
    separated.fold(Vec::new(), |mut blocks, block| {
        match (blocks.last_mut(), block) {
            (Some(Block::Lines(lines)), Block::Lines(more)) => lines.extend(more),
            (_, block) => blocks.push(block),
        }
        blocks
    })
}

pub(super) fn is_empty(group: &[Block]) -> bool {
    group.iter().all(|block| matches!(block, Block::Lines(lines) if lines.is_empty()))
}

/// Puts `first` before the first line and `rest` before every other; a picture cannot be indented and is left as is.
pub(super) fn prefix(blocks: Vec<Block>, first: &[Span], rest: &[Span]) -> Vec<Block> {
    let mut is_first_line = true;
    blocks
        .into_iter()
        .map(|block| match block {
            Block::Lines(lines) => {
                let lines = lines.into_iter().map(|line| prefix_line(line, if std::mem::take(&mut is_first_line) { first } else { rest }));
                Block::Lines(lines.collect())
            }
            Block::Picture(picture) => {
                is_first_line = false;
                Block::Picture(picture)
            }
        })
        .collect()
}

/// `prefix` for bare lines.
pub(super) fn prefix_lines(lines: Vec<Line>, first: &[Span], rest: &[Span]) -> Vec<Line> {
    lines.into_iter().enumerate().map(|(index, line)| prefix_line(line, if index == 0 { first } else { rest })).collect()
}

fn prefix_line(line: Line, prefix: &[Span]) -> Line {
    let prefix = if line.spans.is_empty() { trim_end(prefix) } else { prefix.to_vec() };
    Line::new(prefix.into_iter().chain(line.spans).filter(|span| !span.text.is_empty()).collect())
}

fn trim_end(spans: &[Span]) -> Vec<Span> {
    let mut trimmed = spans.to_vec();
    while trimmed.last().is_some_and(|span| span.text.trim_end().is_empty()) {
        trimmed.pop();
    }
    if let Some(last) = trimmed.last_mut() {
        last.text = last.text.trim_end().to_owned();
    }
    trimmed
}

/// Restyles every span of every line, pictures untouched.
pub(super) fn restyle(blocks: Vec<Block>, change: impl Fn(Style) -> Style) -> Vec<Block> {
    let restyle_line = |line: Line| Line::new(line.spans.into_iter().map(|span| Span { style: change(span.style), ..span }).collect());
    blocks
        .into_iter()
        .map(|block| match block {
            Block::Lines(lines) => Block::Lines(lines.into_iter().map(restyle_line).collect()),
            picture @ Block::Picture(_) => picture,
        })
        .collect()
}

pub(super) fn lines(lines: Vec<Line>) -> Vec<Block> {
    vec![Block::Lines(lines)]
}

/// Spaces as wide as `width`, the continuation indent under a marker.
pub(super) fn blank(width: usize) -> Vec<Span> {
    vec![Span::plain(" ".repeat(width))]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(line: &str) -> Block {
        Block::Lines(vec![Line::new(vec![Span::plain(line)])])
    }

    fn plain(blocks: &[Block]) -> Vec<String> {
        blocks
            .iter()
            .flat_map(|block| match block {
                Block::Lines(lines) => lines.iter().map(Line::plain).collect(),
                Block::Picture(_) => vec!["[picture]".to_owned()],
            })
            .collect()
    }

    #[test]
    fn loose_stack_puts_one_blank_line_between_groups_and_skips_empty_ones() {
        let stacked = stack([vec![text("a")], vec![Block::Lines(Vec::new())], vec![text("b")]], Spacing::Loose);

        assert_eq!(plain(&stacked), ["a", "", "b"]);
        assert_eq!(stacked.len(), 1);
    }

    #[test]
    fn tight_stack_has_no_gap() {
        assert_eq!(plain(&stack([vec![text("a")], vec![text("b")]], Spacing::Tight)), ["a", "b"]);
    }

    #[test]
    fn prefix_marks_the_first_line_and_indents_the_rest_leaving_blank_lines_blank() {
        let blocks = stack([vec![text("a")], vec![text("b")]], Spacing::Loose);
        let prefixed = prefix(blocks, &[Span::plain("- ")], &[Span::plain("  ")]);

        assert_eq!(plain(&prefixed), ["- a", "", "  b"]);
    }

    #[test]
    fn prefix_trims_a_bar_on_blank_lines() {
        let blocks = stack([vec![text("a")], vec![text("b")]], Spacing::Loose);

        assert_eq!(plain(&prefix(blocks, &[Span::plain("│ ")], &[Span::plain("│ ")])), ["│ a", "│", "│ b"]);
    }
}
