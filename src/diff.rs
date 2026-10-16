//! Pairs the sections of two versions of a Markdown file and marks the words that changed.

use std::collections::VecDeque;
use std::mem::discriminant;
use std::ops::Range;

use similar::{Algorithm, DiffOp, DiffTag, capture_diff_slices};
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{Block, Line, Settings};
use crate::markdown::{Section, SectionKind, render_sections};

/// Both diffs take up to the product of the two sides' lengths. Past this many sections, both sides counted, only
/// the equal sections at the start and the end pair as `Same`.
const MAX_SECTIONS: usize = 10_000;
/// A changed pair with more words than this, both sides counted, is marked whole without a word diff.
const MAX_WORDS: usize = 20_000;
/// The words one call diffs across all its changed pairs; a pair that does not fit what is left is marked whole.
const MAX_DIFFED_WORDS: usize = 50_000;

/// What happened to one section between the old and the new version.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SectionChange {
    /// Same kind and same text on both sides, but for the frame and the width of runs of spaces.
    Same {
        old: Section,
        new: Section,
    },
    Added(Section),
    Removed(Section),
    /// Same kind, different text: `old_words` / `new_words` are char ranges into each side's plain text
    /// (`lines.iter().map(Line::plain).collect::<String>()` over its `Block::Lines` and `Block::DoubleHeight`), ready for
    /// `document::highlight`. Empty for sections with no words to mark (pictures, Mermaid diagrams, a change of
    /// spacing alone).
    Changed {
        old: Section,
        new: Section,
        old_words: Vec<Range<usize>>,
        new_words: Vec<Range<usize>>,
    },
}

/// Both files rendered with `render_sections(_, settings)` then paired, in reading order.
pub fn sections(old: &str, new: &str, settings: &Settings) -> Vec<SectionChange> {
    let old = render_sections(old, settings);
    let new = render_sections(new, settings);
    let ops = section_ops(&old, &new);
    paired(&ops, old, new)
}

/// What a section is compared by: its kind, the input's text of each line, and the pixels of its pictures, since a
/// picture's alt text only names the kind of diagram.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Key<'a> {
    kind: &'a SectionKind,
    lines: Vec<String>,
    pictures: Vec<&'a [u8]>,
}

impl<'a> Key<'a> {
    fn of(section: &'a Section) -> Self {
        let pictures =
            section.blocks.iter().filter_map(|block| if let Block::Picture(picture) = block { Some(&picture.png[..]) } else { None });
        let keeps_spacing = keeps_spacing(&section.kind);
        let lines = lines(section).map(|line| written(line, keeps_spacing)).collect();
        Self { kind: &section.kind, lines, pictures: pictures.collect() }
    }
}

/// Code, Mermaid source, front matter and HTML show the input's spacing; elsewhere a run of spaces is the layout's.
fn keeps_spacing(kind: &SectionKind) -> bool {
    matches!(kind, SectionKind::Code { .. } | SectionKind::Mermaid | SectionKind::FrontMatter | SectionKind::Html)
}

fn lines(section: &Section) -> impl Iterator<Item = &Line> {
    section.blocks.iter().flat_map(Block::lines)
}

/// The input's text of a line, each piece of frame a space; runs of spaces made one, unless the spacing is the input's.
fn written(line: &Line, keeps_spacing: bool) -> String {
    let text: String = line.spans.iter().map(|span| if span.is_frame { " " } else { span.text.as_str() }).collect();
    if keeps_spacing { text } else { text.split_whitespace().collect::<Vec<_>>().join(" ") }
}

fn section_ops(old: &[Section], new: &[Section]) -> Vec<DiffOp> {
    let old_keys: Vec<Key> = old.iter().map(Key::of).collect();
    let new_keys: Vec<Key> = new.iter().map(Key::of).collect();
    if old_keys.len() + new_keys.len() > MAX_SECTIONS {
        return ends_only(&old_keys, &new_keys);
    }
    capture_diff_slices(Algorithm::Patience, &old_keys, &new_keys)
}

/// The equal items at the start and at the end, everything between one replaced run.
fn ends_only<T: PartialEq>(old: &[T], new: &[T]) -> Vec<DiffOp> {
    let start = old.iter().zip(new).take_while(|(old, new)| old == new).count();
    let end = old[start..].iter().rev().zip(new[start..].iter().rev()).take_while(|(old, new)| old == new).count();
    let (old_len, new_len) = (old.len() - start - end, new.len() - start - end);
    let ops = [
        DiffOp::Equal { old_index: 0, new_index: 0, len: start },
        DiffOp::Replace { old_index: start, old_len, new_index: start, new_len },
        DiffOp::Equal { old_index: start + old_len, new_index: start + new_len, len: end },
    ];
    ops.into_iter().filter(|op| op.old_range().len() + op.new_range().len() > 0).collect()
}

fn is_equal(op: &DiffOp) -> bool {
    op.tag() == DiffTag::Equal
}

/// Each side's sections taken in the order `ops` cover them, both sides from start to end: an equal run pairs them
/// one to one, a differing run as `pair` says, the word budget passed from one run to the next.
fn paired(ops: &[DiffOp], old: Vec<Section>, new: Vec<Section>) -> Vec<SectionChange> {
    let (mut old, mut new) = (old.into_iter(), new.into_iter());
    let mut changes = Vec::new();
    let mut budget = MAX_DIFFED_WORDS;
    for run in ops.chunk_by(|left, right| is_equal(left) == is_equal(right)) {
        let removed: Vec<Section> = old.by_ref().take(run.iter().map(|op| op.old_range().len()).sum()).collect();
        let added: Vec<Section> = new.by_ref().take(run.iter().map(|op| op.new_range().len()).sum()).collect();
        if run.iter().all(is_equal) {
            changes.extend(removed.into_iter().zip(added).map(|(old, new)| SectionChange::Same { old, new }));
        } else {
            let (pairs, left) = pair(removed, added, budget);
            changes.extend(pairs);
            budget = left;
        }
    }
    changes
}

/// A heading that changed level, a task that was ticked or a fence that changed language is still the same section.
fn pairs_with(old: &SectionKind, new: &SectionKind) -> bool {
    discriminant(old) == discriminant(new)
}

/// Pairs each removed section, in order, with the next added section of its kind; added sections it skips stay added.
/// Gives back the word budget left.
fn pair(removed: Vec<Section>, added: Vec<Section>, budget: usize) -> (Vec<SectionChange>, usize) {
    let mut added = VecDeque::from(added);
    let mut changes = Vec::new();
    let mut budget = budget;
    // Sections only leave `added`, so a kind with no partner left never finds one: each kind is searched in vain once.
    let mut kinds_left_alone = Vec::new();
    for old in removed {
        let kind = discriminant(&old.kind);
        let partner = if kinds_left_alone.contains(&kind) { None } else { added.iter().position(|new| pairs_with(&old.kind, &new.kind)) };
        let Some(skipped) = partner else {
            kinds_left_alone.push(kind);
            changes.push(SectionChange::Removed(old));
            continue;
        };
        changes.extend(added.drain(..skipped).map(SectionChange::Added));
        let Some(new) = added.pop_front() else {
            changes.push(SectionChange::Removed(old));
            continue;
        };
        let (change, left) = changed(old, new, budget);
        changes.push(change);
        budget = left;
    }
    changes.extend(added.into_iter().map(SectionChange::Added));
    (changes, budget)
}

/// The pair with its changed words marked, and the word budget left.
fn changed(old: Section, new: Section, budget: usize) -> (SectionChange, usize) {
    let (old_words, new_words, budget) =
        if old.kind == SectionKind::Mermaid { (Vec::new(), Vec::new(), budget) } else { changed_words(&words(&old), &words(&new), budget) };
    (SectionChange::Changed { old, new, old_words, new_words }, budget)
}

/// A word or a punctuation mark of a section's text, the chars it covers in the lines joined, and whether only
/// spaces separate it from a word before it on its line.
struct Word<'a> {
    text: &'a str,
    chars: Range<usize>,
    joins_previous: bool,
}

/// The words of the input's text, frame left out: a frame between two words, or a line end, keeps them apart.
fn words(section: &Section) -> Vec<Word<'_>> {
    let mut words = Vec::new();
    let mut next = 0;
    for line in lines(section) {
        let mut joins_previous = false;
        for span in &line.spans {
            if span.is_frame {
                next += span.text.chars().count();
                joins_previous = false;
                continue;
            }
            for text in span.text.split_word_bounds() {
                let chars = next..next + text.chars().count();
                next = chars.end;
                if !text.chars().all(char::is_whitespace) {
                    words.push(Word { text, chars, joins_previous });
                    joins_previous = true;
                }
            }
        }
    }
    words
}

/// Each side's ranges, and the word budget left: a pair over `MAX_WORDS` or the budget is marked whole, undiffed.
fn changed_words(old: &[Word], new: &[Word], budget: usize) -> (Vec<Range<usize>>, Vec<Range<usize>>, usize) {
    let count = old.len() + new.len();
    let (ops, budget) = if count > MAX_WORDS || count > budget {
        (vec![DiffOp::Replace { old_index: 0, old_len: old.len(), new_index: 0, new_len: new.len() }], budget)
    } else {
        (capture_diff_slices(Algorithm::Myers, &texts(old), &texts(new)), budget - count)
    };
    (marked(&ops, old, DiffOp::old_range), marked(&ops, new, DiffOp::new_range), budget)
}

fn texts<'a>(words: &[Word<'a>]) -> Vec<&'a str> {
    words.iter().map(|word| word.text).collect()
}

/// The chars of each run of changed words on one line, the spaces between them included, never a frame.
fn marked(ops: &[DiffOp], words: &[Word], side: fn(&DiffOp) -> Range<usize>) -> Vec<Range<usize>> {
    let changed = ops.iter().flat_map(|op| std::iter::repeat_n(!is_equal(op), side(op).len()));
    let flagged: Vec<(&Word, bool)> = words.iter().zip(changed).collect();
    let runs = flagged.chunk_by(|(_, left), (word, right)| word.joins_previous && left == right);
    let changed_runs = runs.filter(|run| run.iter().all(|(_, changed)| *changed));
    changed_runs.filter_map(|run| Some(run.first()?.0.chars.start..run.last()?.0.chars.end)).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::document::{CellSize, test_settings};

    const README: &str = "# mrk\n\n## Install\n\nRun the installer once.\n\n- fast\n- safe\n\nThat is all.\n";
    const MERMAID: &str = "```mermaid\ngraph TD\n  A --> B\n```\n";

    fn diff(old: &str, new: &str) -> Vec<SectionChange> {
        sections(old, new, &test_settings())
    }

    fn diff_at(old: &str, new: &str, width: usize) -> Vec<SectionChange> {
        sections(old, new, &Settings { width, ..test_settings() })
    }

    fn outcomes(changes: &[SectionChange]) -> Vec<&'static str> {
        let outcome = |change: &SectionChange| match change {
            SectionChange::Same { .. } => "same",
            SectionChange::Added(_) => "added",
            SectionChange::Removed(_) => "removed",
            SectionChange::Changed { .. } => "changed",
        };
        changes.iter().map(outcome).collect()
    }

    fn sides(changes: Vec<SectionChange>) -> (Vec<Section>, Vec<Section>) {
        let (mut old, mut new) = (Vec::new(), Vec::new());
        for change in changes {
            match change {
                SectionChange::Same { old: before, new: after } | SectionChange::Changed { old: before, new: after, .. } => {
                    old.push(before);
                    new.push(after);
                }
                SectionChange::Added(after) => new.push(after),
                SectionChange::Removed(before) => old.push(before),
            }
        }
        (old, new)
    }

    /// The text each range covers in the section's plain text.
    fn marked_text(section: &Section, ranges: &[Range<usize>]) -> Vec<String> {
        let text: Vec<char> = lines(section).map(Line::plain).collect::<String>().chars().collect();
        ranges.iter().map(|range| text[range.clone()].iter().collect()).collect()
    }

    /// The text marked on each side of the one change `changes` holds.
    fn marked_words(changes: &[SectionChange]) -> (Vec<String>, Vec<String>) {
        let [SectionChange::Changed { old, new, old_words, new_words }] = changes else { panic!("{changes:#?}") };
        (marked_text(old, old_words), marked_text(new, new_words))
    }

    fn texts(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_owned()).collect()
    }

    #[test]
    fn an_edited_readme_gives_one_change_of_each_sort_and_keeps_the_rest() {
        let new = README.replace("## Install\n\n", "").replace("once", "twice").replace("- safe\n", "- safe\n- small\n");

        assert_eq!(outcomes(&diff(README, &new)), ["same", "removed", "changed", "same", "same", "added", "same"]);
    }

    #[test]
    fn one_word_changed_in_a_wrapped_paragraph_marks_that_word_on_each_side() {
        let old = "The quick brown fox jumps over the lazy dog, then runs far into the woods and over the hills, where nobody can see.\n";
        let new = old.replace("quick", "slow");

        assert_eq!(marked_words(&diff_at(old, &new, 30)), (texts(&["quick"]), texts(&["slow"])));
    }

    #[test]
    fn neighbouring_changed_words_are_one_range_without_the_spaces_around() {
        let changes = diff("Keep this text and that.\n", "Keep other words and that.\n");

        assert_eq!(marked_words(&changes), (texts(&["this text"]), texts(&["other words"])));
    }

    #[test]
    fn a_quote_that_wraps_again_marks_its_word_and_never_its_bar() {
        let old = "> The quick brown fox jumps over the lazy dog, then runs far into the woods and over the hills.\n";
        let new = old.replace("quick", "very slow");

        assert_eq!(marked_words(&diff_at(old, &new, 30)), (texts(&["quick"]), texts(&["very slow"])));
    }

    #[test]
    fn changed_words_across_a_line_end_are_marked_line_by_line_without_the_frame() {
        let quote = "> one two three four five six seven eight nine ten\n";
        let code = "```\nalpha beta gamma delta epsilon zeta\n```\n";

        assert_eq!(marked_words(&diff_at(quote, &quote.replace("four five", "4 5"), 20)).0, ["four", "five"]);
        assert_eq!(marked_words(&diff_at(code, &code.replace("gamma delta", "G D"), 20)).0, ["gamma", "delta epsilon"]);
    }

    #[test]
    fn cells_side_by_side_are_marked_apart_from_the_border_between_them() {
        let changes = diff("| a | b |\n|---|---|\n| one | two |\n", "| a | b |\n|---|---|\n| uno | dos |\n");

        assert_eq!(marked_words(&changes), (texts(&["one", "two"]), texts(&["uno", "dos"])));
    }

    #[test]
    fn a_list_renumbered_wider_keeps_its_items_the_same() {
        let nine: String = (1..=9).map(|number| format!("{number}. item\n")).collect();
        let ten = format!("{nine}10. item\n");

        assert_eq!(outcomes(&diff(&nine, &ten)), [vec!["same"; 9], vec!["added"]].concat());
    }

    #[test]
    fn spacing_in_code_is_a_change() {
        assert_eq!(outcomes(&diff("```\nif a:\n  b\n```\n", "```\nif a:\n    b\n```\n")), ["changed"]);
    }

    #[test]
    fn spacing_inside_a_line_and_written_box_characters_are_changes() {
        for (old, new) in [("a+b\n", "a + b\n"), ("f(x)\n", "f (x)\n"), ("a, b\n", "a ,b\n"), ("a \u{2500} b\n", "a b\n")] {
            assert_eq!(outcomes(&diff(old, new)), ["changed"], "{old:?} -> {new:?}");
        }
        assert_eq!(marked_words(&diff("see \u{2502} here\n", "see here\n")).0, ["\u{2502}"]);
    }

    #[test]
    fn a_paragraph_rewrapped_with_wider_spaces_is_the_same() {
        assert_eq!(outcomes(&diff("one  two\nthree\n", "one two three\n")), ["same"]);
    }

    #[test]
    fn past_the_section_limit_only_equal_ends_pair_as_same() {
        let ops = ends_only(&[1, 2, 3, 4], &[1, 9, 8, 4]);

        assert_eq!(
            ops.iter().map(|op| (op.tag(), op.old_range(), op.new_range())).collect::<Vec<_>>(),
            [(DiffTag::Equal, 0..1, 0..1), (DiffTag::Replace, 1..3, 1..3), (DiffTag::Equal, 3..4, 3..4)]
        );
        assert_eq!(ends_only(&[1, 2], &[1, 2]).len(), 1);
    }

    #[test]
    fn a_pair_over_what_is_left_of_the_word_budget_is_marked_whole_and_a_smaller_one_after_it_diffed() {
        let paragraph = |word: &str| format!("{}\n\n", "word ".repeat(MAX_WORDS / 2 - 1) + word);
        let old = ["a", "b", "c"].map(paragraph).concat() + "short a";
        let new = ["x", "y", "z"].map(paragraph).concat() + "short x";

        let changes = diff_at(&old, &new, 1000);

        let marks: Vec<usize> =
            changes.iter().map(|change| if let SectionChange::Changed { old_words, .. } = change { old_words.len() } else { 0 }).collect();
        assert_eq!(marks[..2], [1, 1]);
        assert!(marks[2] > 1, "{marks:?}");
        let SectionChange::Changed { old_words, .. } = &changes[3] else { panic!("a changed short paragraph") };
        assert_eq!(old_words.first(), Some(&(6..7)), "{old_words:?}");
    }

    #[test]
    fn the_same_input_gives_the_same_output() {
        let new = README.replace("once", "twice") + "\n" + MERMAID;

        assert_eq!(diff(README, &new), diff(README, &new));
    }

    #[test]
    fn a_paragraph_over_the_word_limit_is_marked_whole() {
        let old = "word ".repeat(MAX_WORDS / 2 + 1);
        let new = old.replacen("word", "other", 1);

        let changes = diff_at(&old, &new, 1000);

        let [SectionChange::Changed { old, old_words, .. }] = &changes[..] else { panic!("one change") };
        assert_eq!(old_words.len(), old.blocks.iter().flat_map(Block::lines).count());
    }

    #[test]
    fn a_changed_mermaid_section_marks_no_words() {
        let new = MERMAID.replace('B', "C");
        for cell in [None, Some(CellSize { width_px: 10, height_px: 22 })] {
            let changes = sections(MERMAID, &new, &Settings { cell, ..test_settings() });

            assert!(
                matches!(&changes[..], [SectionChange::Changed { old_words, new_words, .. }] if old_words.is_empty() && new_words.is_empty()),
                "cell {cell:?}: {changes:#?}"
            );
        }
    }

    #[test]
    fn a_line_break_added_inside_a_paragraph_is_a_change() {
        assert_eq!(outcomes(&diff("one two\n", "one\\\ntwo\n")), ["changed"]);
    }

    #[test]
    fn a_heading_that_changes_level_or_a_ticked_task_is_changed() {
        assert_eq!(outcomes(&diff("# Title\n", "## Title\n")), ["changed"]);
        assert_eq!(outcomes(&diff("- [ ] ship\n", "- [x] ship\n")), ["changed"]);
    }

    #[test]
    fn a_removed_section_pairs_with_the_next_added_section_of_its_kind() {
        assert_eq!(outcomes(&diff("# Title\n\nOld text.\n", "- new\n\nNew text.\n")), ["removed", "added", "changed"]);
    }

    #[test]
    fn every_section_of_each_side_comes_up_once_in_order() {
        let pairs = [("- a\n- b\n", "- b\n- a\n"), (README, "- safe\n- fast\n\n# mrk\n"), ("", README), (README, ""), (README, README)];
        for (old, new) in pairs {
            let (before, after) = sides(diff(old, new));

            assert_eq!(before, render_sections(old, &test_settings()), "{old:?} -> {new:?}");
            assert_eq!(after, render_sections(new, &test_settings()), "{old:?} -> {new:?}");
        }
    }

    #[test]
    fn an_empty_side_gives_only_added_or_removed_sections() {
        assert!(diff("", README).iter().all(|change| matches!(change, SectionChange::Added(_))));
        assert!(diff(README, "").iter().all(|change| matches!(change, SectionChange::Removed(_))));
        assert_eq!(diff(README, README).len(), 6);
    }

    #[test]
    fn an_item_inserted_above_renumbers_the_rest_and_keeps_them_the_same() {
        assert_eq!(outcomes(&diff("1. a\n2. b\n", "1. new\n2. a\n3. b\n")), ["added", "same", "same"]);
    }
}
