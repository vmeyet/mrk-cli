//! The values `--color`, `--images`, `--align`, `MRK_ALIGN` and the config file take; the library's enums know no parser.
use clap::ValueEnum;
use mrk::terminal::{Align, ColorChoice, ImagesMode};
use serde::Deserialize;

/// `auto`, `always` or `never`: when colours or images are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum When {
    #[default]
    Auto,
    Always,
    Never,
}

impl From<When> for ColorChoice {
    fn from(when: When) -> Self {
        match when {
            When::Auto => Self::Auto,
            When::Always => Self::Always,
            When::Never => Self::Never,
        }
    }
}

impl From<When> for ImagesMode {
    fn from(when: When) -> Self {
        match when {
            When::Auto => Self::Auto,
            When::Always => Self::Always,
            When::Never => Self::Never,
        }
    }
}

/// `left` or `center`: where the text sits in the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Where {
    Left,
    Center,
}

impl From<Where> for Align {
    fn from(place: Where) -> Self {
        match place {
            Where::Left => Self::Left,
            Where::Center => Self::Center,
        }
    }
}
