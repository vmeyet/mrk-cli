//! Pairs the sections of two versions of a Markdown file and marks the words that changed.

use std::collections::VecDeque;
use std::mem::discriminant;
use std::ops::Range;
use std::time::{Duration, Instant};

use similar::{Algorithm, DiffOp, DiffTag, capture_diff_slices_deadline};
use unicode_segmentation::UnicodeSegmentation;

use crate::document::{Block, Line, Settings};
use crate::markdown::{Section, SectionKind, render_sections};

/// Both diffs can take the product of the two sides' lengths: past this, what is left to compare is marked whole.
const DEADLINE: Duration = Duration::from_secs(1);
/// A pair of sections with more words than this, both sides counted, is marked whole without a word diff.
const MAX_WORDS: usize = 20_000;

/// What happened to one section between the old and the new version.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SectionChange {
    /// Same kind and same words on both sides; spacing and frame (bars, borders, marker padding) may differ.
    Same {
        old: Section,
        new: Section,
    },
    Added(Section),
    Removed(Section),
    /// Same kind, different text: `old_words` / `new_words` are char ranges into each side's plain text
    /// (`lines.iter().map(Line::plain).collect::<String>()` over its `Block::Lines` and `Block::DoubleHeight`), ready for
    /// `document::highlight`. Empty for sections with no words to mark (pictures, Mermaid diagrams, a change of
    /// spacing in code, front matter or HTML).
    Changed {
        old: Section,
        new: Section,
        old_words: Vec<Range<usize>>,
        new_words: Vec<Range<usize>>,
    },
}

/// Both files rendered with `render_sections(_, settings)` then paired, in reading order.
pub fn sections(old: &str, new: &str, settings: &Settings) -> Vec<SectionChange> {
    sections_within(old, new, settings, Instant::now() + DEADLINE)
}

fn sections_within(old: &str, new: &str, settings: &Settings, deadline: Instant) -> Vec<SectionChange> {
    let old = render_sections(old, settings);
    let new = render_sections(new, settings);
    let ops = section_ops(&old, &new, deadline);
    paired(&ops, old, new, deadline)
}

/// What a section is compared by: its kind, its lines, and the pixels of its pictures, since a picture's alt text
/// only names the kind of diagram. Lines are compared by their words, except where spacing is content.
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
        let lines = plain_lines(section);
        let lines = if keeps_spacing(&section.kind) {
            lines
        } else {
            lines.iter().map(|line| texts(&words(std::slice::from_ref(line))).join(" ")).collect()
        };
        Self { kind: &section.kind, lines, pictures: pictures.collect() }
    }
}

/// Code, Mermaid source, front matter and HTML show the input's spacing; elsewhere spacing is the layout's.
fn keeps_spacing(kind: &SectionKind) -> bool {
    matches!(kind, SectionKind::Code { .. } | SectionKind::Mermaid | SectionKind::FrontMatter | SectionKind::Html)
}

fn section_ops(old: &[Section], new: &[Section], deadline: Instant) -> Vec<DiffOp> {
    let old_keys: Vec<Key> = old.iter().map(Key::of).collect();
    let new_keys: Vec<Key> = new.iter().map(Key::of).collect();
    capture_diff_slices_deadline(Algorithm::Patience, &old_keys, &new_keys, Some(deadline))
}

fn is_equal(op: &DiffOp) -> bool {
    op.tag() == DiffTag::Equal
}

/// Each side's sections taken in the order `ops` cover them, both sides from start to end: an equal run pairs them
/// one to one, a differing run as `pair` says.
fn paired(ops: &[DiffOp], old: Vec<Section>, new: Vec<Section>, deadline: Instant) -> Vec<SectionChange> {
    let (mut old, mut new) = (old.into_iter(), new.into_iter());
    let mut changes = Vec::new();
    for run in ops.chunk_by(|left, right| is_equal(left) == is_equal(right)) {
        let removed: Vec<Section> = old.by_ref().take(run.iter().map(|op| op.old_range().len()).sum()).collect();
        let added: Vec<Section> = new.by_ref().take(run.iter().map(|op| op.new_range().len()).sum()).collect();
        if run.iter().all(is_equal) {
            changes.extend(removed.into_iter().zip(added).map(|(old, new)| SectionChange::Same { old, new }));
        } else {
            changes.extend(pair(removed, added, deadline));
        }
    }
    changes
}

/// A heading that changed level, a task that was ticked or a fence that changed language is still the same section.
fn pairs_with(old: &SectionKind, new: &SectionKind) -> bool {
    discriminant(old) == discriminant(new)
}

/// Pairs each removed section, in order, with the next added section of its kind; added sections it skips stay added.
fn pair(removed: Vec<Section>, added: Vec<Section>, deadline: Instant) -> Vec<SectionChange> {
    let mut added = VecDeque::from(added);
    let mut changes = Vec::new();
    for old in removed {
        let Some(skipped) = added.iter().position(|new| pairs_with(&old.kind, &new.kind)) else {
            changes.push(SectionChange::Removed(old));
            continue;
        };
        changes.extend(added.drain(..skipped).map(SectionChange::Added));
        changes.push(match added.pop_front() {
            Some(new) => changed(old, new, deadline),
            None => SectionChange::Removed(old),
        });
    }
    changes.extend(added.into_iter().map(SectionChange::Added));
    changes
}

fn changed(old: Section, new: Section, deadline: Instant) -> SectionChange {
    let (old_words, new_words) = if old.kind == SectionKind::Mermaid {
        (Vec::new(), Vec::new())
    } else {
        changed_words(&plain_lines(&old), &plain_lines(&new), deadline)
    };
    SectionChange::Changed { old, new, old_words, new_words }
}

fn plain_lines(section: &Section) -> Vec<String> {
    section.blocks.iter().flat_map(Block::lines).map(Line::plain).collect()
}

/// Spaces and the bars, borders and continuation signs drawn around text move when it wraps again: diffing them would
/// pull words out of line, and marking them would light up the frame.
fn is_frame(character: char) -> bool {
    matches!(character, '\u{2500}'..='\u{259f}' | '↪')
}

fn is_word(text: &str) -> bool {
    text.chars().any(|character| !character.is_whitespace() && !is_frame(character))
}

/// A word or a punctuation mark of a section's text, the chars it covers in the lines joined, and whether only
/// spaces separate it from a word before it on its line.
struct Word<'a> {
    text: &'a str,
    chars: Range<usize>,
    joins_previous: bool,
}

/// Words never span two lines, so a wrapped paragraph's last word on a row stays apart from the next row's first.
fn words(lines: &[String]) -> Vec<Word<'_>> {
    let mut words = Vec::new();
    let mut next = 0;
    for line in lines {
        let mut joins_previous = false;
        for text in line.split_word_bounds() {
            let chars = next..next + text.chars().count();
            next = chars.end;
            if is_word(text) {
                words.push(Word { text, chars, joins_previous });
                joins_previous = true;
            } else if !text.chars().all(char::is_whitespace) {
                joins_previous = false;
            }
        }
    }
    words
}

fn changed_words(old: &[String], new: &[String], deadline: Instant) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let old = words(old);
    let new = words(new);
    let ops = word_ops(&old, &new, deadline);
    (marked(&ops, &old, DiffOp::old_range), marked(&ops, &new, DiffOp::new_range))
}

fn word_ops(old: &[Word], new: &[Word], deadline: Instant) -> Vec<DiffOp> {
    if old.len() + new.len() > MAX_WORDS {
        return vec![DiffOp::Replace { old_index: 0, old_len: old.len(), new_index: 0, new_len: new.len() }];
    }
    capture_diff_slices_deadline(Algorithm::Myers, &texts(old), &texts(new), Some(deadline))
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
        let text: Vec<char> = plain_lines(section).concat().chars().collect();
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
    fn past_the_deadline_what_differs_is_marked_whole() {
        let (old, new) = ("a b c d\n", "a x c y\n");
        let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();

        assert_eq!(marked_words(&diff(old, new)), (texts(&["b", "d"]), texts(&["x", "y"])));
        assert_eq!(marked_words(&sections_within(old, new, &test_settings(), expired)), (texts(&["b c d"]), texts(&["x c y"])));
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
}
