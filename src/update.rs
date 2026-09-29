//! `mrk update`: where a newer mrk comes from, and whether the running one is already it.
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::version;

/// Where `mrk update` installs from.
pub const REPO: &str = "https://github.com/vmeyet/mrk-cli";

/// cargo's build folder, kept between updates so only mrk recompiles.
const BUILD_FOLDER: &str = "cargo_target";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Install,
    UpToDate,
}

/// A release number, `major.minor.patch`; pre-releases and build metadata are not releases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
}

impl Version {
    fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split('.').map(number);
        let version = Self { major: parts.next()??, minor: parts.next()??, patch: parts.next()?? };
        parts.next().is_none().then_some(version)
    }

    /// The version this binary was built as.
    fn running() -> Result<Self> {
        Self::parse(env!("CARGO_PKG_VERSION")).context("the running version is not a release number")
    }

    /// The git tag a release is published under.
    fn tag(self) -> String {
        format!("v{self}")
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Digits only: `u64::from_str` would also take a leading `+`.
fn number(part: &str) -> Option<u64> {
    part.bytes().all(|byte| byte.is_ascii_digit()).then(|| part.parse().ok()).flatten()
}

/// Only a release newer than the running one installs, unless forced.
pub fn decide(installed: Version, latest: Version, force: bool) -> Decision {
    if force || latest > installed { Decision::Install } else { Decision::UpToDate }
}

/// The newest release among `git ls-remote --tags` lines (`<commit>\trefs/tags/v1.2.3`).
fn newest_release(listing: &str) -> Option<Version> {
    listing.lines().filter_map(|line| line.split_once("\trefs/tags/v")).filter_map(|(_, release)| Version::parse(release)).max()
}

fn cache_root() -> PathBuf {
    std::env::var_os("MRK_CACHE_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::cache_dir().map(|dir| dir.join("mrk")))
        .unwrap_or_else(|| PathBuf::from(".mrk-cache"))
}

fn build_folder() -> Result<PathBuf> {
    let path = cache_root().join(BUILD_FOLDER);
    std::fs::create_dir_all(&path).with_context(|| format!("creating {}", path.display()))?;
    Ok(path)
}

fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git").args(args).env("GIT_TERMINAL_PROMPT", "0").output().context("running git")?;
    if !output.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// The newest release tagged in the repo.
fn latest() -> Result<Version> {
    let listing = git(&["ls-remote", "--tags", "--refs", REPO])?;
    newest_release(&listing).with_context(|| format!("{REPO} has no release tag"))
}

/// cargo ties a `--git` build to the repo URL, never to the commit, so a kept build folder would
/// look fresh and reinstall the old binary with its old commit hash. Dropping mrk's own
/// fingerprints recompiles mrk and reruns `build.rs`; the dependencies stay built.
fn forget_mrk(build: &Path) -> Result<()> {
    let Ok(entries) = std::fs::read_dir(build.join("release").join(".fingerprint")) else { return Ok(()) };
    let prefix = concat!(env!("CARGO_PKG_NAME"), "-");
    for entry in entries {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(prefix) {
            std::fs::remove_dir_all(entry.path()).with_context(|| format!("removing {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Rebuilds and installs a release from the repo with cargo, reusing the dependencies built last time.
fn install(release: Version) -> Result<()> {
    let build = build_folder()?;
    forget_mrk(&build)?;
    let status = Command::new("cargo")
        .args(["install", "--git", REPO, "--tag", &release.tag(), "--force", "--locked", "--target-dir"])
        .arg(&build)
        .status()
        .context("running cargo install")?;
    if !status.success() {
        bail!("cargo install failed");
    }
    Ok(())
}

/// Rebuilds and installs the latest release when it is newer than the running one.
pub fn run(force: bool) -> Result<()> {
    let latest = latest().context("could not check the latest release")?;
    match decide(Version::running()?, latest, force) {
        Decision::UpToDate => println!("✓ already up to date ({})", version::label()),
        Decision::Install => {
            println!("→ installing mrk {} from {REPO}…", latest.tag());
            install(latest)?;
            println!("✓ updated, run `mrk --version` to see it");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn forgetting_mrk_drops_only_its_own_fingerprints() {
        let build = tempfile::tempdir().unwrap();
        let fingerprints = build.path().join("release").join(".fingerprint");
        for name in ["mrk-3483aceb8f1a5a28", "mrk-e613dd567bc7d97f", "serde-0123456789abcdef", "mrkdown-0000000000000000"] {
            std::fs::create_dir_all(fingerprints.join(name)).unwrap();
        }

        forget_mrk(build.path()).unwrap();

        let mut left: Vec<String> =
            std::fs::read_dir(&fingerprints).unwrap().map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["mrkdown-0000000000000000", "serde-0123456789abcdef"]);
    }

    #[test]
    fn forgetting_mrk_in_a_folder_never_built_is_fine() {
        let build = tempfile::tempdir().unwrap();

        forget_mrk(&build.path().join("missing")).unwrap();
        forget_mrk(build.path()).unwrap();
    }

    #[test]
    fn the_same_release_needs_no_install() {
        assert_eq!(decide(version("0.2.0"), version("0.2.0"), false), Decision::UpToDate);
    }

    #[test]
    fn an_older_release_needs_no_install() {
        assert_eq!(decide(version("0.3.0"), version("0.2.9"), false), Decision::UpToDate);
    }

    #[test]
    fn a_newer_release_installs() {
        assert_eq!(decide(version("0.2.0"), version("0.10.0"), false), Decision::Install);
    }

    #[test]
    fn force_installs_over_the_same_release() {
        assert_eq!(decide(version("0.2.0"), version("0.2.0"), true), Decision::Install);
    }

    #[test]
    fn versions_parse_only_plain_release_numbers() {
        assert_eq!(version("1.22.3").to_string(), "1.22.3");
        for text in ["1.2", "1.2.3.4", "1.2.3-rc.1", "1.2.3+build", "1.+2.3", "v1.2.3", "1.x.3", ""] {
            assert_eq!(Version::parse(text), None, "{text}");
        }
    }

    #[test]
    fn the_running_version_is_a_release() {
        assert_eq!(Version::running().unwrap().to_string(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn a_release_is_tagged_with_a_v() {
        assert_eq!(version("0.2.0").tag(), "v0.2.0");
    }

    #[test]
    fn the_newest_release_wins_by_number_not_by_listing_order() {
        let listing =
            "aaaa\trefs/tags/v0.10.0\nbbbb\trefs/tags/v0.9.1\ncccc\trefs/tags/v1.0.0-rc.1\ndddd\trefs/tags/nightly\neeee\trefs/tags/v0.2.0";

        assert_eq!(newest_release(listing), Some(version("0.10.0")));
    }

    #[test]
    fn a_repo_without_release_tags_has_no_newest_release() {
        assert_eq!(newest_release(""), None);
        assert_eq!(newest_release("aaaa\trefs/tags/nightly"), None);
    }
}
