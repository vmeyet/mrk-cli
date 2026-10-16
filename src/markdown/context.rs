use crate::document::{MIN_WIDTH, Settings, Style};
use crate::theme::Palette;

/// A container indents its blocks only while that leaves them this wide.
const MIN_NESTED_WIDTH: usize = 12;
/// Containers nested deeper than this render their content as bare text: bounded recursion on hostile input.
const MAX_NESTING: usize = 32;

/// Where a block is being rendered: the settings at the current width, the paragraph style, and how deep it sits.
#[derive(Clone, Copy, Debug)]
pub(super) struct Context {
    pub settings: Settings,
    pub style: Style,
    pub nesting: usize,
    pub list_depth: usize,
}

/// How a container lays out its blocks.
pub(super) enum Nested {
    /// Behind the container's prefix, in a context that much narrower.
    Indented(Context),
    /// At the container's own width without its prefix, since indenting would leave less than `MIN_NESTED_WIDTH`.
    Unindented(Context),
    /// Past `MAX_NESTING`: the container renders as its bare text.
    TooDeep,
}

impl Context {
    /// The context of the whole document, at least `MIN_WIDTH` wide.
    pub fn new(settings: &Settings) -> Self {
        let settings = Settings { width: settings.width.max(MIN_WIDTH), ..*settings };
        Self { settings, style: Style::fg(settings.theme.palette.text), nesting: 0, list_depth: 0 }
    }

    pub fn palette(&self) -> &Palette {
        &self.settings.theme.palette
    }

    pub fn width(&self) -> usize {
        self.settings.width
    }

    /// The context inside a container whose prefix takes `columns` cells.
    pub fn nested(&self, columns: usize) -> Nested {
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
