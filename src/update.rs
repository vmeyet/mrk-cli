//! `mrk update`: where a newer mrk comes from, and whether the running one is already it.
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use mrk::sanitize;
use signal_hook::consts::{SIGINT, SIGTERM};

use crate::cli::signals::{self, Held};
use crate::version;

/// Where `mrk update` installs from.
pub const REPO: &str = "https://github.com/vmeyet/mrk-cli";

const BINARY: &str = "mrk";

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

impl Release {
    /// `0.7.0 (a1b2c3d)`, as `mrk --version` shows it.
    fn label(&self) -> String {
        format!("{} ({})", self.version, version::short(&self.commit))
    }
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
fn stdout_of(mut command: Command) -> Result<String> {
    let words: Vec<_> = std::iter::once(command.get_program()).chain(command.get_args()).map(|word| word.to_string_lossy()).collect();
    let line = words.join(" ");
    let output = command.output().with_context(|| format!("running {line}"))?;
    if !output.status.success() {
        bail!("{line}: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// The newest release tagged in the repo.
fn latest() -> Result<Release> {
    let mut git = Command::new("git");
    git.args(["ls-remote", "--tags", REPO]).env("GIT_TERMINAL_PROMPT", "0");
    newest_release(&stdout_of(git)?).with_context(|| format!("{REPO} has no release tag"))
}

/// Homebrew keeps every formula it installs under a `Cellar` folder.
fn is_brew_path(exe: &Path) -> bool {
    exe.components().any(|part| part.as_os_str() == "Cellar")
}

fn installed_by_brew() -> bool {
    std::env::current_exe().and_then(std::fs::canonicalize).is_ok_and(|exe| is_brew_path(&exe))
}

/// Whether `brew list --versions` (`mrk 0.6.0 0.7.0_1`, `_1` being a formula revision) lists the release.
fn brew_lists(listing: &str, release: Version) -> bool {
    let versions = listing.split_whitespace().skip(1).map(|installed| installed.split_once('_').map_or(installed, |(version, _)| version));
    versions.filter_map(Version::parse).any(|installed| installed == release)
}

fn run_brew(arguments: &[&str]) -> Result<()> {
    let line = arguments.join(" ");
    println!("→ brew {line}…");
    let status = Command::new("brew").args(arguments).status().with_context(|| format!("running brew {line}"))?;
    if !status.success() {
        bail!("brew {line} failed");
    }
    Ok(())
}

/// brew reads the tap as it last fetched it, up to a day old, so it is fetched first unless the user turned that off.
/// `brew upgrade` does nothing for the version already installed, so a forced update reinstalls. The tap's formula
/// follows a release once its build is done, so brew can succeed and still install an older version.
fn install_with_brew(release: Version, force: bool) -> Result<()> {
    if std::env::var_os("HOMEBREW_NO_AUTO_UPDATE").is_some_and(|value| !value.is_empty()) {
        println!("→ HOMEBREW_NO_AUTO_UPDATE is set: brew reads the tap as it last fetched it, which may miss the release");
    } else {
        run_brew(&["update"])?;
    }
    run_brew(&[if force { "reinstall" } else { "upgrade" }, FORMULA])?;
    let mut list = Command::new("brew");
    list.args(["list", "--versions", FORMULA]);
    let listing = stdout_of(list)?;
    if !brew_lists(&listing, release) {
        bail!("brew installed {listing}, not {}: the tap's formula has not caught up with the release yet, try again later", release.tag());
    }
    Ok(())
}

/// Builds the release's commit in a temporary folder, whatever `CARGO_TARGET_DIR` says, removed once done, also when
/// Ctrl-C or `SIGTERM` stops the build; installs it where the running mrk is, then checks what was installed and
/// which `mrk` the `PATH` finds.
fn install_with_cargo(release: &Release) -> Result<()> {
    println!("→ installing mrk {} from {REPO}…", release.label());
    let root = install_root().context("finding where to install mrk")?;
    let held = Held::hold(&[SIGINT, SIGTERM]).context("holding Ctrl-C during the build")?;
    let build = tempfile::Builder::new().prefix("mrk-update-").tempdir().context("creating a build folder")?;
    let mut cargo = Command::new("cargo");
    cargo
        .args(["install", "--git", REPO, "--rev", &release.commit, "--force", "--locked", "--root"])
        .arg(&root)
        .arg("--target-dir")
        .arg(build.path());
    let mut cargo = signals::spawn_alone(&mut cargo).context("running cargo install")?;
    let status = match signals::wait_ending(&mut cargo, &held).context("waiting for cargo install")? {
        Ok(status) => status,
        Err(interrupted) => {
            println!("→ stopped: nothing was installed, the build folder is removed");
            return Err(interrupted.into());
        }
    };
    if !status.success() {
        bail!("cargo install failed");
    }
    check_cargo_install(release, &root.join("bin").join(BINARY))
}

/// The folder whose `bin` holds the running mrk, so the update replaces it; else where cargo installs by default.
fn install_root() -> Option<PathBuf> {
    let running = std::env::current_exe().and_then(std::fs::canonicalize).ok();
    running
        .as_deref()
        .and_then(root_of)
        .or_else(|| cargo_root(std::env::var_os("CARGO_INSTALL_ROOT"), std::env::var_os("CARGO_HOME"), std::env::home_dir()))
}

/// `…/bin/mrk` is installed under `…`.
fn root_of(binary: &Path) -> Option<PathBuf> {
    let folder = binary.parent()?;
    (folder.file_name()? == "bin").then(|| folder.parent().map(Path::to_path_buf)).flatten()
}

/// What the running mrk now reports must be the release, and it must be the `mrk` a shell runs.
fn check_cargo_install(release: &Release, installed: &Path) -> Result<()> {
    let mut ask = Command::new(installed);
    ask.arg("--version");
    let label = stdout_of(ask)?;
    if !reports(&label, release) {
        bail!("{} reports {label}, not {}", sanitize::text(&installed.display().to_string()), release.label());
    }
    match on_path(&std::env::var_os("PATH").unwrap_or_default()) {
        Some(found) if is_same_file(&found, installed) => {}
        Some(found) => {
            println!(
                "! `mrk` runs {}, not the one installed: put {} first in PATH",
                sanitize::text(&found.display().to_string()),
                parent(installed)
            );
        }
        None => println!("! {} is not in PATH", parent(installed)),
    }
    Ok(())
}

/// Where `cargo install` puts binaries by default: `$CARGO_INSTALL_ROOT`, else `$CARGO_HOME`, else `~/.cargo`.
fn cargo_root(install_root: Option<OsString>, cargo_home: Option<OsString>, home: Option<PathBuf>) -> Option<PathBuf> {
    install_root.or(cargo_home).map(PathBuf::from).or_else(|| home.map(|home| home.join(".cargo")))
}

/// The first `mrk` among the `PATH` folders, as a shell would run it.
fn on_path(path: &OsStr) -> Option<PathBuf> {
    std::env::split_paths(path).map(|folder| folder.join(BINARY)).find(|candidate| candidate.is_file())
}

fn is_same_file(one: &Path, other: &Path) -> bool {
    matches!((std::fs::canonicalize(one), std::fs::canonicalize(other)), (Ok(one), Ok(other)) if one == other)
}

fn parent(binary: &Path) -> String {
    sanitize::text(&binary.parent().unwrap_or(binary).display().to_string())
}

/// `mrk --version` prints `mrk 0.7.0 (a1b2c3d)`, the commit being the one built.
fn reports(label: &str, release: &Release) -> bool {
    label == format!("{BINARY} {}", release.label())
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

    #[test]
    fn cargo_installs_under_its_install_root_then_its_home_then_dot_cargo() {
        let home = Some(PathBuf::from("/Users/nina"));

        assert_eq!(cargo_root(Some("/opt/root".into()), Some("/opt/cargo".into()), home.clone()), Some(PathBuf::from("/opt/root")));
        assert_eq!(cargo_root(None, Some("/opt/cargo".into()), home.clone()), Some(PathBuf::from("/opt/cargo")));
        assert_eq!(cargo_root(None, None, home), Some(PathBuf::from("/Users/nina/.cargo")));
        assert_eq!(cargo_root(None, None, None), None);
    }

    #[test]
    fn the_update_installs_where_the_running_binary_is() {
        assert_eq!(root_of(Path::new("/Users/nina/tools/bin/mrk")), Some(PathBuf::from("/Users/nina/tools")));
        assert_eq!(root_of(Path::new("/Users/nina/target/release/mrk")), None);
    }

    #[test]
    fn the_first_mrk_in_path_is_the_one_a_shell_runs() {
        let root = tempfile::tempdir().unwrap();
        let folders = ["empty", "first", "second"].map(|name| root.path().join(name));
        for folder in &folders {
            std::fs::create_dir_all(folder).unwrap();
        }
        for folder in &folders[1..] {
            std::fs::write(folder.join(BINARY), b"").unwrap();
        }

        let path = std::env::join_paths(&folders).unwrap();

        assert_eq!(on_path(&path), Some(folders[1].join(BINARY)));
        assert_eq!(on_path(&std::env::join_paths(&folders[..1]).unwrap()), None);
    }

    #[test]
    fn the_installed_binary_must_report_the_release_and_its_commit() {
        let installing = release("0.7.0", COMMIT);

        assert!(reports("mrk 0.7.0 (2222222)", &installing));
        assert!(!reports("mrk 0.7.0 (unknown)", &installing));
        assert!(!reports("mrk 0.6.0 (2222222)", &installing));
    }
}
