use std::collections::HashMap;

use super::language;

/// syntect's time on a line can grow with the square of its length (a JavaScript regex literal: 8 KB in 0.8 s,
/// 32 KB in 15 s), so a line costs its length once per started `LINE_COST_STEP` bytes: shorter lines cost their
/// length, and the worst time of a document's highlighting stays what 256 KiB of such lines take.
const LINE_COST_STEP: usize = 512;
/// What the code one document highlights may cost, every block counted in reading order: past it, blocks are plain.
const MAX_DOCUMENT_COST: usize = 256 * 1024;

/// How much of the start of a block is highlighted, and what highlighting it costs its document.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Highlighting {
    len: usize,
    cost: usize,
}

/// The lines of `code` up to the first that costs more than a whole document may, none when no syntax is found for
/// its language.
pub(crate) fn highlighting(code: &str, language: Option<&str>) -> Highlighting {
    if language.and_then(language::find).is_none() {
        return Highlighting::default();
    }
    let lines = code.split_inclusive('\n').map(|line| (line.len(), line_cost(line)));
    let (len, cost) =
        lines.take_while(|(_, cost)| *cost <= MAX_DOCUMENT_COST).fold((0, 0), |(len, cost), line| (len + line.0, cost + line.1));
    Highlighting { len, cost }
}

fn line_cost(line: &str) -> usize {
    let steps = line.trim_end_matches(['\n', '\r']).len().div_ceil(LINE_COST_STEP).max(1);
    line.len() * steps
}

/// The code blocks of one document its budget lets highlight, each that fits what is left of it in reading order,
/// known by a key unique to the block (the line it starts on).
pub(crate) struct Allowance(HashMap<usize, usize>);

impl Allowance {
    /// `blocks` are each block's key and `highlighting`, in reading order.
    pub(crate) fn new(blocks: impl IntoIterator<Item = (usize, Highlighting)>) -> Self {
        let mut admitted = HashMap::new();
        let mut left = MAX_DOCUMENT_COST;
        for (key, highlighting) in blocks {
            if highlighting.cost <= left {
                left -= highlighting.cost;
                admitted.insert(key, highlighting.len);
            }
        }
        Self(admitted)
    }

    /// How much of the block's start is highlighted: the `len` of its `highlighting` when admitted, else nothing.
    pub(crate) fn highlighted(&self, key: usize) -> usize {
        self.0.get(&key).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(len: usize) -> String {
        format!("{}\n", "a".repeat(len))
    }

    #[test]
    fn code_naming_a_syntax_is_highlighted_whole() {
        assert_eq!(highlighting("let a = 1;\r\nb", Some("rust")), Highlighting { len: 13, cost: 13 });
    }

    #[test]
    fn code_without_a_syntax_is_not_highlighted() {
        for language in [None, Some("text"), Some("TXT"), Some("output"), Some("rsut")] {
            assert_eq!(highlighting("a\n", language), Highlighting::default(), "{language:?}");
        }
    }

    #[test]
    fn a_line_costs_its_length_once_per_started_step() {
        assert_eq!(line_cost(&line(LINE_COST_STEP)), LINE_COST_STEP + 1);
        assert_eq!(line_cost(&line(LINE_COST_STEP + 1)), 2 * (LINE_COST_STEP + 2));
        assert_eq!(line_cost(&line(10 * LINE_COST_STEP)), 10 * (10 * LINE_COST_STEP + 1));
    }

    #[test]
    fn a_long_string_literal_leaves_the_rest_of_its_block_highlighted() {
        let code = format!("fn main() {{\n    let s = \"{}\";\n    let x = 1;\n}}\n", "a".repeat(600));

        assert_eq!(highlighting(&code, Some("rust")).len, code.len());
    }

    #[test]
    fn highlighting_stops_at_the_first_line_costing_more_than_a_document_may() {
        let (fitting, too_long) = (line(11_000), line(12_000));
        assert!(line_cost(&fitting) <= MAX_DOCUMENT_COST && line_cost(&too_long) > MAX_DOCUMENT_COST);
        let code = format!("x\n{fitting}{too_long}y\n");

        assert_eq!(highlighting(&code, Some("js")).len, 2 + fitting.len());
    }

    #[test]
    fn each_block_that_fits_what_is_left_is_admitted_in_reading_order() {
        let half = |len: usize| Highlighting { len, cost: MAX_DOCUMENT_COST / 2 };
        let allowance =
            Allowance::new([(1, half(1)), (2, Highlighting { len: 2, cost: MAX_DOCUMENT_COST / 2 + 1 }), (3, half(3)), (4, half(4))]);

        assert_eq!([1, 2, 3, 4].map(|key| allowance.highlighted(key)), [1, 0, 3, 0]);
    }
}
