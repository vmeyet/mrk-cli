//! `mrk update`: where a newer mrk comes from, and whether the running one is already it.
use std::fmt;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use mrk::version;

/// Where `mrk update` installs from.
pub const REPO: &str = "https://github.com/vmeyet/mrk-cli";

/// The Homebrew formula `mrk update` upgrades when brew installed the running binary.
const FORMULA: &str = "vmeyet/tap/mrk";

const PEELED_SUFFIX: &str = "^{}";

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

/// A release and the commit its tag pointed at when listed: cargo builds that commit, so a tag moved since cannot
/// swap the code.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Release {
    version: Version,
    commit: String,
}

/// One `git ls-remote --tags` line: `<id>\trefs/tags/v1.2.3`, then `<commit>\trefs/tags/v1.2.3^{}` when the tag is
/// annotated, its own id being the tag object's rather than the commit's.
struct Tag<'a> {
    version: Version,
    id: &'a str,
    is_peeled: bool,
}

fn tag(line: &str) -> Option<Tag<'_>> {
    let (id, name) = line.split_once("\trefs/tags/v")?;
    let (name, is_peeled) = name.strip_suffix(PEELED_SUFFIX).map_or((name, false), |name| (name, true));
    let version = Version::parse(name)?;
    is_object_id(id).then_some(Tag { version, id, is_peeled })
}

/// A full SHA-1 or SHA-256 id, so what reaches cargo's `--rev` is never a flag nor a branch name.
fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// The newest release among `git ls-remote --tags` lines, with the commit its tag points at.
fn newest_release(listing: &str) -> Option<Release> {
    let newest = listing.lines().filter_map(tag).max_by_key(|tag| (tag.version, tag.is_peeled))?;
    Some(Release { version: newest.version, commit: newest.id.to_owned() })
}

/// Only a release newer than the running one installs, unless forced.
pub fn decide(installed: Version, latest: Version, force: bool) -> Decision {
    if force || latest > installed { Decision::Install } else { Decision::UpToDate }
}

/// The trimmed stdout of a command that must succeed.
fn stdout_of(command: &mut Command) -> Result<String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command.output().with_context(|| format!("running {program}"))?;
    if !output.status.success() {
        bail!("{program} failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// The newest release tagged in the repo.
fn latest() -> Result<Release> {
    let listing = stdout_of(Command::new("git").args(["ls-remote", "--tags", REPO]).env("GIT_TERMINAL_PROMPT", "0"))?;
    newest_release(&listing).with_context(|| format!("{REPO} has no release tag"))
}

/// Homebrew keeps every formula it installs under a `Cellar` folder.
fn is_brew_path(exe: &Path) -> bool {
    exe.components().any(|part| part.as_os_str() == "Cellar")
}

fn installed_by_brew() -> bool {
    std::env::current_exe().and_then(std::fs::canonicalize).is_ok_and(|exe| is_brew_path(&exe))
}

/// `brew upgrade` does nothing for the version already installed, so a forced update reinstalls.
fn brew_command(force: bool) -> &'static str {
    if force { "reinstall" } else { "upgrade" }
}

/// Whether `brew list --versions` (`mrk 0.6.0 0.7.0_1`, `_1` being a formula revision) lists the release.
fn brew_lists(listing: &str, release: Version) -> bool {
    let versions = listing.split_whitespace().skip(1).map(|installed| installed.split_once('_').map_or(installed, |(version, _)| version));
    versions.filter_map(Version::parse).any(|installed| installed == release)
}

/// The tap's formula follows a release by a few minutes, so brew can succeed and still install an older version.
fn install_with_brew(release: Version, force: bool) -> Result<()> {
    let command = brew_command(force);
    println!("→ brew {command} {FORMULA}…");
    let status = Command::new("brew").args([command, FORMULA]).status().with_context(|| format!("running brew {command}"))?;
    if !status.success() {
        bail!("brew {command} failed");
    }
    let listing = stdout_of(Command::new("brew").args(["list", "--versions", FORMULA]))?;
    if !brew_lists(&listing, release) {
        bail!("brew installed {listing}, not {}: the tap has not caught up yet, try again in a few minutes", release.tag());
    }
    Ok(())
}

/// Builds the release's commit in a temporary folder cargo removes once installed.
fn install_with_cargo(release: &Release) -> Result<()> {
    println!("→ installing mrk {} ({}) from {REPO}…", release.version.tag(), version::short(&release.commit));
    let status = Command::new("cargo")
        .args(["install", "--git", REPO, "--rev", &release.commit, "--force", "--locked"])
        .status()
        .context("running cargo install")?;
    if !status.success() {
        bail!("cargo install failed");
    }
    Ok(())
}

/// Installs through brew when brew installed mrk, otherwise builds the release with cargo.
fn install(release: &Release, force: bool) -> Result<()> {
    if installed_by_brew() { install_with_brew(release.version, force) } else { install_with_cargo(release) }
}

/// Installs the latest release when it is newer than the running one.
pub fn run(force: bool) -> Result<()> {
    let latest = latest().context("could not check the latest release")?;
    match decide(Version::running()?, latest.version, force) {
        Decision::UpToDate => println!("✓ already up to date ({})", version::label()),
        Decision::Install => {
            install(&latest, force)?;
            println!("✓ installed mrk {}, run `mrk --version` to see it", latest.version.tag());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const TAG_OBJECT: &str = "1111111111111111111111111111111111111111";
    const COMMIT: &str = "2222222222222222222222222222222222222222";
    const OLD_COMMIT: &str = "3333333333333333333333333333333333333333";

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn release(text: &str, commit: &str) -> Release {
        Release { version: version(text), commit: commit.to_owned() }
    }

    #[test]
    fn a_binary_in_the_brew_cellar_is_a_brew_install() {
        assert!(is_brew_path(Path::new("/opt/homebrew/Cellar/mrk/0.3.0/bin/mrk")));
    }

    #[test]
    fn a_binary_installed_by_cargo_is_not_a_brew_install() {
        assert!(!is_brew_path(Path::new("/Users/me/.cargo/bin/mrk")));
    }

    #[test]
    fn brew_reinstalls_when_forced_and_upgrades_otherwise() {
        assert_eq!(brew_command(true), "reinstall");
        assert_eq!(brew_command(false), "upgrade");
    }

    #[test]
    fn brew_has_the_release_when_it_lists_it_whatever_the_formula_revision() {
        assert!(brew_lists("mrk 0.7.0", version("0.7.0")));
        assert!(brew_lists("mrk 0.6.0 0.7.0_1", version("0.7.0")));
    }

    #[test]
    fn brew_lacks_the_release_when_the_tap_has_not_caught_up() {
        assert!(!brew_lists("mrk 0.6.0", version("0.7.0")));
        assert!(!brew_lists("", version("0.7.0")));
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
        let listing = format!(
            "{OLD_COMMIT}\trefs/tags/v0.10.0\n{COMMIT}\trefs/tags/v0.9.1\n{COMMIT}\trefs/tags/v1.0.0-rc.1\n{COMMIT}\trefs/tags/nightly\n{COMMIT}\trefs/tags/v0.2.0"
        );

        assert_eq!(newest_release(&listing), Some(release("0.10.0", OLD_COMMIT)));
    }

    #[test]
    fn an_annotated_tag_installs_the_commit_it_points_at() {
        let listing = format!("{TAG_OBJECT}\trefs/tags/v0.7.0\n{COMMIT}\trefs/tags/v0.7.0^{{}}\n{OLD_COMMIT}\trefs/tags/v0.6.0");

        assert_eq!(newest_release(&listing), Some(release("0.7.0", COMMIT)));
    }

    #[test]
    fn a_tag_without_a_full_object_id_is_ignored() {
        let listing = format!("--upload-pack=x\trefs/tags/v9.0.0\nabcd\trefs/tags/v8.0.0\n{COMMIT}\trefs/tags/v0.7.0");

        assert_eq!(newest_release(&listing), Some(release("0.7.0", COMMIT)));
    }

    #[test]
    fn a_repo_without_release_tags_has_no_newest_release() {
        assert_eq!(newest_release(""), None);
        assert_eq!(newest_release(&format!("{COMMIT}\trefs/tags/nightly")), None);
    }
}
