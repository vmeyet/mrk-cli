//! `~/.config/mrk/config.toml`.
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::cli::UsageError;
use crate::terminal::{Align, ImagesMode};

pub const MIN_WIDTH: usize = 20;
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

fn check(config: &Config) -> Result<()> {
    match config.width {
        Some(width) if width < MIN_WIDTH => bail!("`width = {width}` is too narrow: {MIN_WIDTH} is the least"),
        _ => Ok(()),
    }
}

pub fn parse(text: &str) -> Result<Config> {
    let config: Config = toml::from_str(text)?;
    check(&config)?;
    Ok(config)
}

pub fn load_from(path: &Path) -> Result<Config> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text).with_context(|| UsageError(format!("invalid config {}", path.display())))
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
