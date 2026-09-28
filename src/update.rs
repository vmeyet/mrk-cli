//! `mrk update`: where a newer mrk comes from, and whether the running one is already it.
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

/// Only a commit we know we already run spares the rebuild; anything unanswered installs.
pub fn decide(installed: &str, latest: Option<&str>, force: bool) -> Decision {
    match latest {
        Some(latest) if !force && installed != version::UNKNOWN && latest == installed => Decision::UpToDate,
        _ => Decision::Install,
    }
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

/// The newest commit of the repo.
fn latest() -> Result<String> {
    let listing = git(&["ls-remote", REPO, "HEAD"])?;
    listing.split_whitespace().next().map(str::to_owned).with_context(|| format!("{REPO} has no HEAD"))
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

/// Rebuilds and installs from the repo with cargo, reusing the dependencies built last time.
fn install() -> Result<()> {
    let build = build_folder()?;
    forget_mrk(&build)?;
    let status = Command::new("cargo")
        .args(["install", "--git", REPO, "--force", "--locked", "--target-dir"])
        .arg(&build)
        .status()
        .context("running cargo install")?;
    if !status.success() {
        bail!("cargo install failed");
    }
    Ok(())
}

/// Rebuilds and installs mrk when the repo moved past the running commit.
pub fn run(force: bool) -> Result<()> {
    let latest = if force { None } else { latest().inspect_err(|error| eprintln!("! could not check the latest version: {error}")).ok() };
    match decide(version::COMMIT, latest.as_deref(), force) {
        Decision::UpToDate => println!("✓ already up to date ({})", version::label()),
        Decision::Install => {
            println!("→ installing the latest mrk from {REPO}…");
            install()?;
            println!("✓ updated, run `mrk --version` to see it");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const INSTALLED: &str = "9731436a0e7c4d1b2f3a4b5c6d7e8f9a0b1c2d3e";
    const NEWER: &str = "635cf1b0000000000000000000000000000000ff";

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
    fn same_commit_needs_no_install() {
        assert_eq!(decide(INSTALLED, Some(INSTALLED), false), Decision::UpToDate);
    }

    #[test]
    fn a_newer_commit_installs() {
        assert_eq!(decide(INSTALLED, Some(NEWER), false), Decision::Install);
    }

    #[test]
    fn force_installs_over_the_same_commit() {
        assert_eq!(decide(INSTALLED, Some(INSTALLED), true), Decision::Install);
    }

    #[test]
    fn a_failed_check_installs() {
        assert_eq!(decide(INSTALLED, None, false), Decision::Install);
    }

    #[test]
    fn an_unknown_installed_commit_installs() {
        assert_eq!(decide(version::UNKNOWN, Some(version::UNKNOWN), false), Decision::Install);
    }
}
