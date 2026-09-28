use crate::document::{Settings, Style};
use crate::theme::Palette;

/// Containers nested deeper than this render their content flat: bounded recursion on hostile input.
const MAX_NESTING: usize = 32;
/// A nested block never gets narrower than this; past it the content renders flat at the current width.
const MIN_NESTED_WIDTH: usize = 12;

/// Where a block is being rendered: the settings at the current width, the paragraph style, and how deep it sits.
#[derive(Clone, Debug)]
pub(super) struct Context {
    pub settings: Settings,
    pub text: Style,
    pub nesting: usize,
    pub list_depth: usize,
}

impl Context {
    pub fn new(settings: &Settings) -> Self {
        Self { settings: settings.clone(), text: Style::fg(settings.theme.palette.text), nesting: 0, list_depth: 0 }
    }

    pub fn palette(&self) -> &Palette {
        &self.settings.theme.palette
    }

    pub fn width(&self) -> usize {
        self.settings.width
    }

    /// The context inside a container whose prefix takes `columns` cells; `None` when nesting further would not fit.
    pub fn nested(&self, columns: usize) -> Option<Self> {
        let width = self.width().checked_sub(columns).filter(|width| *width >= MIN_NESTED_WIDTH)?;
        let is_too_deep = self.nesting >= MAX_NESTING;
        if is_too_deep {
            return None;
        }
        Some(Self { settings: Settings { width, ..self.settings.clone() }, nesting: self.nesting + 1, ..self.clone() })
    }

    pub fn with_text(self, text: Style) -> Self {
        Self { text, ..self }
    }

    pub fn in_list(self) -> Self {
        Self { list_depth: self.list_depth + 1, ..self }
    }
}
