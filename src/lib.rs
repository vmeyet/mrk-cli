//! Render Markdown beautifully in the terminal.

#[cfg(feature = "cli")]
pub mod cli;
pub mod code;
#[cfg(feature = "cli")]
pub mod config;
pub mod document;
pub mod markdown;
pub mod mermaid;
#[cfg(feature = "cli")]
pub mod terminal;
pub mod text;
pub mod theme;
#[cfg(feature = "cli")]
pub mod update;
pub mod version;
