//! Render Markdown beautifully in the terminal.
//!
//! The renderer behind the `mrk` command: it turns Markdown into a [`Document`](document::Document) of styled lines,
//! with code highlighted and Mermaid diagrams drawn as text or as PNG pictures. It never prints; writing the document
//! to a terminal is [`terminal`]'s job, behind the default `cli` feature.
//!
//! ```
//! use mrk::document::{self, Settings};
//! use mrk::markdown;
//!
//! let mut settings = Settings::default();
//! settings.width = 40;
//! let rendered = markdown::render("# Notes\n\n- one\n- two\n", &settings);
//! print!("{}", document::printable(&rendered));
//! ```
//!
//! Print only through [`sanitize`], `Line::printable` or `document::printable`: Markdown is untrusted input and a
//! terminal interprets escape sequences. The guide is `docs/library.md` in the repository.
#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod code;
pub mod diff;
pub mod document;
pub mod markdown;
pub mod mermaid;
mod raster;
pub mod sanitize;
#[cfg(all(test, feature = "cli"))]
mod snapshot;
#[cfg(feature = "cli")]
pub mod terminal;
mod text;
pub mod theme;

/// Runs the Rust snippets of `docs/library.md` as doctests, so the guide compiles against the API it describes.
#[cfg(doctest)]
#[doc = include_str!("../docs/library.md")]
struct LibraryGuide;
