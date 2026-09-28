use crate::document::{Line, Settings};

/// A fenced code block as a panel: every line exactly `settings.width` cells, highlighted when the language is known.
pub fn render(code: &str, language: Option<&str>, settings: &Settings) -> Vec<Line> {
    let _ = (code, language, settings);
    todo!("feat/code")
}
