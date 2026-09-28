use std::io::Write;

use crate::document::{CellSize, Document};
use crate::theme::Appearance;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    None,
    Ansi256,
    TrueColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub color: ColorDepth,
    pub hyperlinks: bool,
    /// Set when the terminal draws kitty-protocol pictures and pictures are wanted.
    pub cell: Option<CellSize>,
    pub background: Option<Appearance>,
    pub columns: u16,
}

pub fn write(document: &Document, capabilities: &Capabilities, out: &mut impl Write) -> std::io::Result<()> {
    let _ = (document, capabilities, out);
    todo!("feat/terminal")
}
