//! Render Markdown beautifully in the terminal.

pub mod code;
pub mod diff;
pub mod document;
pub mod markdown;
pub mod mermaid;
pub mod raster;
pub mod sanitize;
#[cfg(feature = "cli")]
pub mod terminal;
pub mod text;
pub mod theme;
pub mod version;
