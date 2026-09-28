use unicode_width::UnicodeWidthStr;

use crate::document::{Line, Span};

pub fn display_width(text: &str) -> usize {
    text.width()
}

/// Breaks styled spans into lines of at most `width` cells, at spaces when it can, mid-word when a word is wider than the line.
/// Every line after the first starts with `indent` (hanging indent for list items and quotes).
pub fn wrap(spans: &[Span], width: usize, indent: &[Span]) -> Vec<Line> {
    let _ = (spans, width, indent);
    todo!("feat/markdown implements wrap")
}
