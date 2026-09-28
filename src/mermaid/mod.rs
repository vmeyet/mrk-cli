use crate::document::{Block, Settings};

/// A ```mermaid block: a picture when `settings.cell` is set and the diagram renders, text otherwise.
pub fn render(source: &str, settings: &Settings) -> Block {
    let _ = (source, settings);
    todo!("feat/mermaid")
}
