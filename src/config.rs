//! `~/.config/mrk/config.toml`.
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use mrk::terminal::{Align, ImagesMode};
use serde::Deserialize;

use crate::cli::UsageError;

pub const MIN_WIDTH: usize = 20;
/// Wider than any window (a 4K screen in a 6-pixel font is about 640 columns): it only rejects absurd widths, such as
/// one whose padding would overflow.
pub const MAX_WIDTH: usize = 1000;
pub const DEFAULT_WIDTH: usize = 100;
const FILE: &str = "mrk/config.toml";

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub theme: Option<String>,
    pub theme_dark: Option<String>,
    pub theme_light: Option<String>,
    pub width: Option<usize>,
    pub images: Option<ImagesMode>,
    pub align: Option<Align>,
    pub pager: Option<bool>,
    pub jumbo_title: Option<bool>,
}

fn candidates(xdg: Option<OsString>, home: Option<PathBuf>, platform: Option<PathBuf>) -> Vec<PathBuf> {
    let xdg = xdg.map(PathBuf::from).filter(|path| path.is_absolute());
    let terminal_style = xdg.or_else(|| home.map(|home| home.join(".config")));
    let mut paths: Vec<PathBuf> = [terminal_style, platform].into_iter().flatten().map(|dir| dir.join(FILE)).collect();
    paths.dedup();
    paths
}

/// Where the config is looked for, in order: `$XDG_CONFIG_HOME` or `~/.config` first, where terminal tools keep
/// theirs on macOS too, then the platform's own directory (`~/Library/Application Support` on macOS).
pub fn paths() -> Vec<PathBuf> {
    candidates(std::env::var_os("XDG_CONFIG_HOME"), dirs::home_dir(), dirs::config_dir())
}

/// The width when it lies within `MIN_WIDTH..=MAX_WIDTH`, else why not, for the flag, the environment and the config alike.
pub fn check_width(width: usize) -> Result<usize, String> {
    match width {
        ..MIN_WIDTH => Err(format!("{width} is too narrow: {MIN_WIDTH} is the least")),
        MIN_WIDTH..=MAX_WIDTH => Ok(width),
        _ => Err(format!("{width} is too wide: {MAX_WIDTH} is the most")),
    }
}

pub fn parse(text: &str) -> Result<Config> {
    let config: Config = toml::from_str(text)?;
    if let Some(width) = config.width {
        check_width(width).map_err(|reason| anyhow!("width {reason}"))?;
    }
    Ok(config)
}

/// A config file that exists but cannot be read is as much a usage mistake as one that does not parse.
pub fn load_from(path: &Path) -> Result<Config> {
    let config = std::fs::read_to_string(path).map_err(anyhow::Error::from).and_then(|text| parse(&text));
    config.with_context(|| UsageError(format!("invalid config {}", path.display())))
}

/// The first config file that exists; defaults when there is none.
pub fn load() -> Result<Config> {
    paths().into_iter().find(|path| path.exists()).map_or_else(|| Ok(Config::default()), |path| load_from(&path))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn home() -> Option<PathBuf> {
        "/Users/nina".parse().ok()
    }

    #[test]
    fn dot_config_comes_before_the_platform_directory() {
        let found = candidates(None, home(), Some(PathBuf::from("/Users/nina/Library/Application Support")));

        assert_eq!(
            found,
            [
                PathBuf::from("/Users/nina/.config/mrk/config.toml"),
                PathBuf::from("/Users/nina/Library/Application Support/mrk/config.toml")
            ]
        );
    }

    #[test]
    fn xdg_config_home_replaces_dot_config_when_absolute() {
        assert_eq!(candidates(Some("/xdg".into()), home(), None), [PathBuf::from("/xdg/mrk/config.toml")]);
        assert_eq!(candidates(Some("relative".into()), home(), None), [PathBuf::from("/Users/nina/.config/mrk/config.toml")]);
    }

    #[test]
    fn the_same_directory_is_listed_once() {
        let found = candidates(None, home(), Some(PathBuf::from("/Users/nina/.config")));

        assert_eq!(found, [PathBuf::from("/Users/nina/.config/mrk/config.toml")]);
    }

    #[test]
    fn every_key_is_read() {
        let config = parse(
            "theme = \"mrk-light\"\ntheme_dark = \"nord\"\ntheme_light = \"github-light\"\nwidth = 72\nimages = \"never\"\nalign = \"left\"\npager = true\njumbo_title = true\n",
        )
        .unwrap();
        let expected = Config {
            theme: Some("mrk-light".to_owned()),
            theme_dark: Some("nord".to_owned()),
            theme_light: Some("github-light".to_owned()),
            width: Some(72),
            images: Some(ImagesMode::Never),
            align: Some(Align::Left),
            pager: Some(true),
            jumbo_title: Some(true),
        };

        assert_eq!(config, expected);
    }

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn an_unknown_key_is_refused_with_its_line() {
        let error = format!("{:#}", parse("theme = \"x\"\ncolour = \"always\"\n").unwrap_err());

        assert!(error.contains("line 2") && error.contains("colour"), "{error}");
    }

    #[test]
    fn a_bad_value_is_refused() {
        assert!(parse("images = \"sometimes\"").is_err());
        assert!(parse("pager = \"yes\"").is_err());
        assert!(parse("jumbo_title = 1").is_err());
        assert!(parse("theme_dark = 1").is_err());

        assert!(parse("width = \"wide\"").is_err());
        assert!(parse("width = 5").unwrap_err().to_string().contains("too narrow"));
        assert!(parse("width = 50000000").unwrap_err().to_string().contains("too wide"));
    }

    #[test]
    fn a_width_is_kept_only_between_the_bounds() {
        assert_eq!(check_width(MIN_WIDTH), Ok(MIN_WIDTH));
        assert_eq!(check_width(MAX_WIDTH), Ok(MAX_WIDTH));
        assert_eq!(check_width(MIN_WIDTH - 1), Err("19 is too narrow: 20 is the least".to_owned()));
        assert_eq!(check_width(usize::MAX).unwrap_err(), format!("{} is too wide: 1000 is the most", usize::MAX));
    }

    #[test]
    fn an_unreadable_config_is_a_usage_error() {
        let folder = tempfile::tempdir().unwrap();

        assert!(load_from(folder.path()).unwrap_err().is::<UsageError>());
    }

    #[test]
    fn a_malformed_file_names_its_path() {
        let path = std::env::temp_dir().join(format!("mrk-config-test-{}.toml", std::process::id()));
        std::fs::write(&path, "theme = \n").unwrap();
        let error = load_from(&path).unwrap_err();
        std::fs::remove_file(&path).unwrap();

        assert!(error.is::<UsageError>());
        let error = format!("{error:#}");
        assert!(error.contains(&path.display().to_string()) && error.contains("line 1"), "{error}");
    }
}
