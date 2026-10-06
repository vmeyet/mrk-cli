//! Pairs the blocks of two versions of a Markdown file and marks the words that changed.

use std::mem::discriminant;
use std::ops::Range;

use similar::{Algorithm, DiffOp, DiffTag, capture_diff_slices};
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{Block, Line, Settings};
use crate::markdown::{BlockKind, SourceBlock, render_blocks};

/// What happened to one block between the old and the new version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockChange {
    /// Same kind and same plain text on both sides.
    Same {
        old: SourceBlock,
        new: SourceBlock,
    },
    Added(SourceBlock),
    Removed(SourceBlock),
    /// Same kind, different text: `old_words` / `new_words` are char ranges into each side's plain text
    /// (`lines.iter().map(Line::plain).collect::<String>()` over its `Block::Lines` and `Block::DoubleHeight`), ready for
    /// `document::highlight`. Empty for blocks with no text to mark (pictures, Mermaid diagrams).
    Changed {
        old: SourceBlock,
        new: SourceBlock,
        old_words: Vec<Range<usize>>,
        new_words: Vec<Range<usize>>,
    },
}

/// Both files rendered with `render_blocks(_, settings)` then paired, in reading order.
pub fn blocks(old: &str, new: &str, settings: &Settings) -> Vec<BlockChange> {
    let old = render_blocks(old, settings);
    let new = render_blocks(new, settings);
    let steps = steps(&old, &new);
    follow(&steps, old, new)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Same,
    Added,
    Removed,
    Changed,
}

/// What a block is compared by: its kind, its text, and the pixels of its pictures, since a picture's alt text only
/// names the kind of diagram.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Key<'a> {
    kind: &'a BlockKind,
    text: String,
    pictures: Vec<&'a [u8]>,
}

impl<'a> Key<'a> {
    fn of(block: &'a SourceBlock) -> Self {
        let pictures =
            block.blocks.iter().filter_map(|part| if let Block::Picture(picture) = part { Some(&picture.png[..]) } else { None });
        Self { kind: &block.kind, text: plain_lines(block).concat(), pictures: pictures.collect() }
    }
}

/// One step per block of either side, in reading order; each side's blocks come up in their own order.
fn steps(old: &[SourceBlock], new: &[SourceBlock]) -> Vec<Step> {
    let old_keys: Vec<Key> = old.iter().map(Key::of).collect();
    let new_keys: Vec<Key> = new.iter().map(Key::of).collect();
    let ops = capture_diff_slices(Algorithm::Patience, &old_keys, &new_keys);
    let runs = ops.chunk_by(|left, right| is_equal(left) == is_equal(right));
    runs.flat_map(|run| {
        if run.first().is_some_and(is_equal) {
            vec![Step::Same; run.iter().map(|op| op.old_range().len()).sum()]
        } else {
            let removed: Vec<&BlockKind> = run.iter().flat_map(DiffOp::old_range).map(|index| &old[index].kind).collect();
            let added: Vec<&BlockKind> = run.iter().flat_map(DiffOp::new_range).map(|index| &new[index].kind).collect();
            pair(&removed, &added)
        }
    })
    .collect()
}

fn is_equal(op: &DiffOp) -> bool {
    op.tag() == DiffTag::Equal
}

/// A heading that changed level, a task that was ticked or a fence that changed language is still the same block.
fn pairs_with(old: &BlockKind, new: &BlockKind) -> bool {
    discriminant(old) == discriminant(new)
}

/// Pairs each removed block, in order, with the next added block of its kind; added blocks it skips stay added.
fn pair(removed: &[&BlockKind], added: &[&BlockKind]) -> Vec<Step> {
    let mut steps = Vec::new();
    let mut next_added = 0;
    for kind in removed {
        match added[next_added..].iter().position(|other| pairs_with(kind, other)) {
            Some(skipped) => {
                steps.extend(std::iter::repeat_n(Step::Added, skipped));
                steps.push(Step::Changed);
                next_added += skipped + 1;
            }
            None => steps.push(Step::Removed),
        }
    }
    steps.extend(std::iter::repeat_n(Step::Added, added.len() - next_added));
    steps
}

/// Hands out each side's blocks as the steps take them; `steps` takes every block once, in order.
fn follow(steps: &[Step], old: Vec<SourceBlock>, new: Vec<SourceBlock>) -> Vec<BlockChange> {
    let mut old = old.into_iter();
    let mut new = new.into_iter();
    let change = |step: &Step| match step {
        Step::Same => Some(BlockChange::Same { old: old.next()?, new: new.next()? }),
        Step::Added => new.next().map(BlockChange::Added),
        Step::Removed => old.next().map(BlockChange::Removed),
        Step::Changed => Some(changed(old.next()?, new.next()?)),
    };
    steps.iter().filter_map(change).collect()
}

fn changed(old: SourceBlock, new: SourceBlock) -> BlockChange {
    let (old_words, new_words) =
        if old.kind == BlockKind::Mermaid { Default::default() } else { words(&plain_lines(&old), &plain_lines(&new)) };
    BlockChange::Changed { old, new, old_words, new_words }
}

fn plain_lines(block: &SourceBlock) -> Vec<String> {
    let lines = block.blocks.iter().flat_map(|part| match part {
        Block::Lines(lines) | Block::DoubleHeight(lines) => lines.as_slice(),
        Block::Picture(_) => &[],
    });
    lines.map(Line::plain).collect()
}

/// A word or a punctuation mark of one line, and the chars it covers in the lines joined.
struct Token<'a> {
    text: &'a str,
    chars: Range<usize>,
}

/// Spaces and the bars and borders drawn around text move when it wraps again: diffing them would pull words out of line.
fn is_written(text: &str) -> bool {
    text.chars().any(|character| !character.is_whitespace() && !matches!(character, '\u{2500}'..='\u{259f}'))
}

/// Tokens never span two lines, so a wrapped paragraph's last word on a row stays apart from the next row's first.
fn tokens(lines: &[String]) -> Vec<Token<'_>> {
    let texts = lines.iter().flat_map(|line| line.split_word_bounds());
    let starts = texts.clone().scan(0, |next, text| {
        let start = *next;
        *next += text.chars().count();
        Some(start..*next)
    });
    texts.zip(starts).filter(|(text, _)| is_written(text)).map(|(text, chars)| Token { text, chars }).collect()
}

fn texts<'a>(tokens: &[Token<'a>]) -> Vec<&'a str> {
    tokens.iter().map(|token| token.text).collect()
}

fn words(old: &[String], new: &[String]) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let old = tokens(old);
    let new = tokens(new);
    let ops = capture_diff_slices(Algorithm::Myers, &texts(&old), &texts(&new));
    (marked(&ops, &old, DiffOp::old_range), marked(&ops, &new, DiffOp::new_range))
}

/// The chars of each run of changed tokens, the spaces between them included.
fn marked(ops: &[DiffOp], tokens: &[Token], side: fn(&DiffOp) -> Range<usize>) -> Vec<Range<usize>> {
    let changed = ops.iter().flat_map(|op| std::iter::repeat_n(!is_equal(op), side(op).len()));
    let flagged: Vec<(&Token, bool)> = tokens.iter().zip(changed).collect();
    let runs = flagged.chunk_by(|(_, left), (_, right)| left == right).filter(|run| run.iter().all(|(_, changed)| *changed));
    runs.filter_map(|run| Some(run.first()?.0.chars.start..run.last()?.0.chars.end)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::CellSize;
    use crate::theme::test_settings;

    const README: &str = "# mrk\n\n## Install\n\nRun the installer once.\n\n- fast\n- safe\n\nThat is all.\n";
    const MERMAID: &str = "```mermaid\ngraph TD\n  A --> B\n```\n";

    fn diff(old: &str, new: &str) -> Vec<BlockChange> {
        blocks(old, new, &test_settings())
    }

    fn steps_of(changes: &[BlockChange]) -> Vec<Step> {
        let step = |change: &BlockChange| match change {
            BlockChange::Same { .. } => Step::Same,
            BlockChange::Added(_) => Step::Added,
            BlockChange::Removed(_) => Step::Removed,
            BlockChange::Changed { .. } => Step::Changed,
        };
        changes.iter().map(step).collect()
    }

    fn sides(changes: Vec<BlockChange>) -> (Vec<SourceBlock>, Vec<SourceBlock>) {
        let (mut old, mut new) = (Vec::new(), Vec::new());
        for change in changes {
            match change {
                BlockChange::Same { old: before, new: after } | BlockChange::Changed { old: before, new: after, .. } => {
                    old.push(before);
                    new.push(after);
                }
                BlockChange::Added(after) => new.push(after),
                BlockChange::Removed(before) => old.push(before),
            }
        }
        (old, new)
    }

    /// The text each range covers in the block's plain text.
    fn marked_text(block: &SourceBlock, ranges: &[Range<usize>]) -> Vec<String> {
        let text: Vec<char> = plain_lines(block).concat().chars().collect();
        ranges.iter().map(|range| text[range.clone()].iter().collect()).collect()
    }

    #[test]
    fn an_edited_readme_gives_one_change_of_each_sort_and_keeps_the_rest() {
        let new = README.replace("## Install\n\n", "").replace("once", "twice").replace("- safe\n", "- safe\n- small\n");

        assert_eq!(
            steps_of(&diff(README, &new)),
            [Step::Same, Step::Removed, Step::Changed, Step::Same, Step::Same, Step::Added, Step::Same]
        );
    }

    #[test]
    fn one_word_changed_in_a_wrapped_paragraph_marks_that_word_on_each_side() {
        let old = "The quick brown fox jumps over the lazy dog, then runs far into the woods and over the hills, where nobody can see.\n";
        let new = old.replace("quick", "slow");
        let settings = crate::document::Settings { width: 30, ..test_settings() };

        let changes = blocks(old, &new, &settings);

        let [BlockChange::Changed { old, new, old_words, new_words }] = &changes[..] else { panic!("{changes:#?}") };
        assert_eq!(marked_text(old, old_words), ["quick"]);
        assert_eq!(marked_text(new, new_words), ["slow"]);
    }

    #[test]
    fn neighbouring_changed_words_are_one_range_without_the_spaces_around() {
        let changes = diff("Keep this text and that.\n", "Keep other words and that.\n");

        let [BlockChange::Changed { old, new, old_words, new_words }] = &changes[..] else { panic!("{changes:#?}") };
        assert_eq!(marked_text(old, old_words), ["this text"]);
        assert_eq!(marked_text(new, new_words), ["other words"]);
    }

    #[test]
    fn a_quote_that_wraps_again_marks_its_word_and_never_its_bar() {
        let old = "> The quick brown fox jumps over the lazy dog, then runs far into the woods and over the hills.\n";
        let new = old.replace("quick", "very slow");
        let settings = crate::document::Settings { width: 30, ..test_settings() };

        let changes = blocks(old, &new, &settings);

        let [BlockChange::Changed { old, new, old_words, new_words }] = &changes[..] else { panic!("{changes:#?}") };
        assert_eq!(marked_text(old, old_words), ["quick"]);
        assert_eq!(marked_text(new, new_words), ["very slow"]);
    }

    #[test]
    fn a_changed_mermaid_block_marks_no_words() {
        let new = MERMAID.replace('B', "C");
        for cell in [None, Some(CellSize { width_px: 10, height_px: 22 })] {
            let changes = blocks(MERMAID, &new, &crate::document::Settings { cell, ..test_settings() });

            assert!(
                matches!(&changes[..], [BlockChange::Changed { old_words, new_words, .. }] if old_words.is_empty() && new_words.is_empty()),
                "cell {cell:?}: {changes:#?}"
            );
        }
    }

    #[test]
    fn a_heading_that_changes_level_or_a_ticked_task_is_changed() {
        assert_eq!(steps_of(&diff("# Title\n", "## Title\n")), [Step::Changed]);
        assert_eq!(steps_of(&diff("- [ ] ship\n", "- [x] ship\n")), [Step::Changed]);
    }

    #[test]
    fn a_removed_block_pairs_with_the_next_added_block_of_its_kind() {
        assert_eq!(steps_of(&diff("# Title\n\nOld text.\n", "- new\n\nNew text.\n")), [Step::Removed, Step::Added, Step::Changed]);
    }

    #[test]
    fn every_block_of_each_side_comes_up_once_in_order() {
        let pairs = [("- a\n- b\n", "- b\n- a\n"), (README, "- safe\n- fast\n\n# mrk\n"), ("", README), (README, ""), (README, README)];
        for (old, new) in pairs {
            let (before, after) = sides(diff(old, new));

            assert_eq!(before, render_blocks(old, &test_settings()), "{old:?} -> {new:?}");
            assert_eq!(after, render_blocks(new, &test_settings()), "{old:?} -> {new:?}");
        }
    }

    #[test]
    fn an_empty_side_gives_only_added_or_removed_blocks() {
        assert!(diff("", README).iter().all(|change| matches!(change, BlockChange::Added(_))));
        assert!(diff(README, "").iter().all(|change| matches!(change, BlockChange::Removed(_))));
        assert_eq!(diff(README, README).len(), 6);
    }

    #[test]
    fn the_same_input_gives_the_same_output() {
        let new = README.replace("once", "twice") + "\n" + MERMAID;

        assert_eq!(diff(README, &new), diff(README, &new));
    }
}
