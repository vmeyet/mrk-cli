//! Render Markdown beautifully in the terminal.

pub mod code;
pub mod diff;
pub mod document;
pub mod markdown;
pub mod mermaid;
pub mod raster;
pub mod sanitize;
#[cfg(all(test, feature = "cli"))]
mod snapshot;
#[cfg(feature = "cli")]
pub mod terminal;
pub mod text;
pub mod theme;

/// Runs the Rust snippets of `docs/library.md` as doctests, so the guide compiles against the API it describes.
#[cfg(doctest)]
#[doc = include_str!("../docs/library.md")]
struct LibraryGuide;
