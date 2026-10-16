//! `MRK_THEME`, `MRK_WIDTH` and `MRK_ALIGN`, read here rather than by clap, which takes `MRK_WIDTH=` for a width.
use anyhow::Result;
use clap::ValueEnum;
use mrk::terminal::Align;

use super::{UsageError, parse_width};

/// What the environment sets, each value below the flags and above the config file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Environment {
    pub theme: Option<String>,
    pub width: Option<usize>,
    pub align: Option<Align>,
}

impl Environment {
    pub fn read() -> Result<Self> {
        Self::from_variables(|name| std::env::var(name).ok())
    }

    /// A variable set but empty counts as unset, as `MRK_THEME= mrk` means.
    fn from_variables(variable: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let value = |name: &str| variable(name).filter(|value| !value.is_empty());
        let width =
            value("MRK_WIDTH").map(|raw| parse_width(&raw).map_err(|reason| UsageError(format!("MRK_WIDTH: {reason}")))).transpose()?;
        let align = value("MRK_ALIGN")
            .map(|raw| Align::from_str(&raw, false).map_err(|_| UsageError(format!("MRK_ALIGN: `{raw}` is neither left nor center"))))
            .transpose()?;
        Ok(Self { theme: value("MRK_THEME"), width, align })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn environment(pairs: &[(&str, &str)]) -> Result<Environment> {
        Environment::from_variables(|name| pairs.iter().find(|(known, _)| *known == name).map(|(_, value)| (*value).to_owned()))
    }

    #[test]
    fn every_variable_is_read() {
        let read = environment(&[("MRK_THEME", "nord"), ("MRK_WIDTH", "72"), ("MRK_ALIGN", "left")]).unwrap();

        assert_eq!(read, Environment { theme: Some("nord".to_owned()), width: Some(72), align: Some(Align::Left) });
    }

    #[test]
    fn a_variable_set_but_empty_counts_as_unset() {
        assert_eq!(environment(&[("MRK_THEME", ""), ("MRK_WIDTH", ""), ("MRK_ALIGN", "")]).unwrap(), Environment::default());
    }

    #[test]
    fn a_bad_value_is_a_usage_error_naming_its_variable() {
        for (name, value, reason) in
            [("MRK_WIDTH", "19", "19 is too narrow"), ("MRK_WIDTH", "wide", "not a number"), ("MRK_ALIGN", "up", "`up`")]
        {
            let error = environment(&[(name, value)]).unwrap_err();

            assert!(error.is::<UsageError>());
            assert!(error.to_string().starts_with(name) && error.to_string().contains(reason), "{error}");
        }
    }
}
