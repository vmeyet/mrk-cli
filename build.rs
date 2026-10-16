//! Embeds the git commit the binary was built from, which `mrk --version` shows.
use std::path::Path;
use std::process::Command;

const UNKNOWN: &str = "unknown";

fn main() {
    let (commit, watched) =
        if is_repository_root() { (git(["rev-parse", "HEAD"]), head_files()) } else { (None, vec!["build.rs".to_owned()]) };
    println!("cargo:rustc-env=GIT_HASH={}", commit.as_deref().unwrap_or(UNKNOWN));
    for path in watched {
        println!("cargo:rerun-if-changed={path}");
    }
}

/// A vendored or packaged copy sits inside some other repository, whose commit says nothing about this code.
fn is_repository_root() -> bool {
    let root = git(["rev-parse", "--show-toplevel"]).and_then(|root| Path::new(&root).canonicalize().ok());
    let package = std::env::var_os("CARGO_MANIFEST_DIR").and_then(|dir| Path::new(&dir).canonicalize().ok());
    root.is_some() && root == package
}

/// What changes when HEAD moves, so the embedded hash never goes stale. The whole `refs` folder is watched because a
/// commit on a packed branch writes a loose ref file that did not exist before.
fn head_files() -> Vec<String> {
    let paths = ["HEAD", "packed-refs", "refs"].map(|name| git(["rev-parse", "--git-path", name]));
    paths.into_iter().flatten().filter(|path| Path::new(path).exists()).collect()
}

fn git<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!text.is_empty()).then_some(text)
}
