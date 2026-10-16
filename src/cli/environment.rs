//! `MRK_THEME`, `MRK_WIDTH` and `MRK_ALIGN`, read here rather than by clap, which takes `MRK_WIDTH=` for a width.
use std::ffi::OsString;

use anyhow::Result;
use clap::ValueEnum;
use mrk::terminal::Align;

use super::{UsageError, parse_width};

/// What the environment sets, each value below the flags and above the config file, read only when no flag sets
/// it: a bad value matters only where it is used.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    theme: Option<OsString>,
    width: Option<OsString>,
    align: Option<OsString>,
}

impl Environment {
    pub fn read() -> Self {
        Self::from_variables(|name| std::env::var_os(name))
    }

    /// A variable set but empty counts as unset, as `MRK_THEME= mrk` means.
    fn from_variables(variable: impl Fn(&str) -> Option<OsString>) -> Self {
        let value = |name: &str| variable(name).filter(|value| !value.is_empty());
        Self { theme: value("MRK_THEME"), width: value("MRK_WIDTH"), align: value("MRK_ALIGN") }
    }

    pub fn theme(&self) -> Result<Option<String>> {
        text("MRK_THEME", self.theme.as_ref())
    }

    pub fn width(&self) -> Result<Option<usize>> {
        let parsed = text("MRK_WIDTH", self.width.as_ref())?.map(|raw| parse_width(&raw));
        Ok(parsed.transpose().map_err(|reason| UsageError(format!("MRK_WIDTH: {reason}")))?)
    }

    pub fn align(&self) -> Result<Option<Align>> {
        let parsed = text("MRK_ALIGN", self.align.as_ref())?.map(|raw| Align::from_str(&raw, false).map_err(|_| raw));
        Ok(parsed.transpose().map_err(|raw| UsageError(format!("MRK_ALIGN: `{raw}` is neither left nor center")))?)
    }
}

fn text(name: &str, value: Option<&OsString>) -> Result<Option<String>> {
    value.map(|value| value.clone().into_string().map_err(|_| UsageError(format!("{name} is not valid UTF-8")).into())).transpose()
}

/// The flag's value, else the environment's, read only then.
pub fn flag_or<T>(flag: Option<T>, environment: impl FnOnce() -> Result<Option<T>>) -> Result<Option<T>> {
    flag.map_or_else(environment, |flag| Ok(Some(flag)))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use std::os::unix::ffi::OsStringExt;

    use super::*;

    fn environment(pairs: &[(&str, &str)]) -> Environment {
        Environment::from_variables(|name| pairs.iter().find(|(known, _)| *known == name).map(|(_, value)| OsString::from(value)))
    }

    #[test]
    fn every_variable_is_read() {
        let read = environment(&[("MRK_THEME", "nord"), ("MRK_WIDTH", "72"), ("MRK_ALIGN", "left")]);

        assert_eq!(read.theme().unwrap().as_deref(), Some("nord"));
        assert_eq!(read.width().unwrap(), Some(72));
        assert_eq!(read.align().unwrap(), Some(Align::Left));
    }

    #[test]
    fn a_variable_set_but_empty_counts_as_unset() {
        assert_eq!(environment(&[("MRK_THEME", ""), ("MRK_WIDTH", ""), ("MRK_ALIGN", "")]), Environment::default());
    }

    #[test]
    fn a_bad_value_is_a_usage_error_naming_its_variable() {
        let bad = environment(&[("MRK_WIDTH", "wide"), ("MRK_ALIGN", "up")]);
        let narrow = environment(&[("MRK_WIDTH", "19")]);
        let not_utf8 = Environment::from_variables(|name| (name == "MRK_THEME").then(|| OsString::from_vec(vec![0xff])));

        for (error, start, reason) in [
            (bad.width().unwrap_err(), "MRK_WIDTH", "not a number"),
            (narrow.width().unwrap_err(), "MRK_WIDTH", "19 is too narrow"),
            (bad.align().unwrap_err(), "MRK_ALIGN", "`up`"),
            (not_utf8.theme().unwrap_err(), "MRK_THEME", "not valid UTF-8"),
        ] {
            assert!(error.is::<UsageError>());
            assert!(error.to_string().starts_with(start) && error.to_string().contains(reason), "{error}");
        }
    }

    #[test]
    fn a_flag_wins_without_reading_a_bad_value() {
        let bad = environment(&[("MRK_WIDTH", "wide")]);

        assert_eq!(flag_or(Some(50), || bad.width()).unwrap(), Some(50));
        assert!(flag_or(None, || bad.width()).is_err());
    }
}
