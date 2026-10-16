use crate::document::{MIN_WIDTH, Settings, Style};
use crate::mermaid::Allowance;
use crate::theme::Palette;

/// A container indents its blocks only while that leaves them this wide.
const MIN_NESTED_WIDTH: usize = 12;
/// Containers nested deeper than this render their content as bare text: bounded recursion on hostile input.
const MAX_NESTING: usize = 32;

/// Where a block is being rendered: the settings at the current width, the paragraph style, how deep it sits, and
/// which of the document's diagrams its budget lets through.
#[derive(Clone, Copy)]
pub(super) struct Context<'a> {
    pub settings: Settings,
    pub style: Style,
    pub nesting: usize,
    pub list_depth: usize,
    pub diagrams: &'a Allowance,
}

/// How a container lays out its blocks.
pub(super) enum Nested<'a> {
    /// Behind the container's prefix, in a context that much narrower.
    Indented(Context<'a>),
    /// At the container's own width without its prefix, since indenting would leave less than `MIN_NESTED_WIDTH`.
    Unindented(Context<'a>),
    /// Past `MAX_NESTING`: the container renders as its bare text.
    TooDeep,
}

impl<'a> Context<'a> {
    /// The context of the whole document, at least `MIN_WIDTH` wide.
    pub fn new(settings: &Settings, diagrams: &'a Allowance) -> Self {
        let settings = Settings { width: settings.width.max(MIN_WIDTH), ..*settings };
        Self { settings, style: Style::fg(settings.theme.palette.text), nesting: 0, list_depth: 0, diagrams }
    }

    pub fn palette(&self) -> &Palette {
        &self.settings.theme.palette
    }

    pub fn width(&self) -> usize {
        self.settings.width
    }

    /// The context inside a container whose prefix takes `columns` cells.
    pub fn nested(&self, columns: usize) -> Nested<'a> {
        if self.nesting >= MAX_NESTING {
            return Nested::TooDeep;
        }
        let deeper = Self { nesting: self.nesting + 1, ..*self };
        match self.width().checked_sub(columns).filter(|width| *width >= MIN_NESTED_WIDTH) {
            Some(width) => Nested::Indented(Self { settings: Settings { width, ..self.settings }, ..deeper }),
            None => Nested::Unindented(deeper),
        }
    }

    pub fn with_style(self, style: Style) -> Self {
        Self { style, ..self }
    }

    pub fn in_list(self) -> Self {
        Self { list_depth: self.list_depth + 1, ..self }
    }
}
