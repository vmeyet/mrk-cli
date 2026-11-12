//! Embeds the git commit the binary was built from, which `mrk --version` shows.
use std::path::Path;
use std::process::Command;

const UNKNOWN: &str = "unknown";
/// Written by `cargo package` at the package root: a crates.io build has no repository to ask.
const VCS_INFO: &str = ".cargo_vcs_info.json";

fn main() {
    let (commit, watched) = if is_repository_root() { (git(["rev-parse", "HEAD"]), head_files()) } else { packaged_commit() };
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

/// Cargo treats a missing watched file as always changed, so `build.rs` stands in when there is no `.cargo_vcs_info.json`.
fn packaged_commit() -> (Option<String>, Vec<String>) {
    match std::fs::read_to_string(VCS_INFO) {
        Ok(vcs_info) => (sha1(&vcs_info), vec![VCS_INFO.to_owned()]),
        Err(_) => (None, vec!["build.rs".to_owned()]),
    }
}

/// The `git.sha1` of `.cargo_vcs_info.json`, read without a JSON dependency: cargo writes that key once, as a hex string.
fn sha1(vcs_info: &str) -> Option<String> {
    let (_, after_key) = vcs_info.split_once("\"sha1\"")?;
    let quoted = after_key.trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let (sha1, _) = quoted.split_once('"')?;
    is_object_id(sha1).then(|| sha1.to_owned())
}

/// The rule of `update::is_object_id`, which the build script cannot import: SHA-1 or SHA-256, in hex.
fn is_object_id(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_reads_the_commit_cargo_package_writes() {
        let vcs_info = "{\n  \"git\": {\n    \"sha1\": \"9731436a0e7c4d1b2f3a4b5c6d7e8f9a0b1c2d3e\",\n    \"dirty\": true\n  },\n  \"path_in_vcs\": \"\"\n}";

        assert_eq!(sha1(vcs_info).as_deref(), Some("9731436a0e7c4d1b2f3a4b5c6d7e8f9a0b1c2d3e"));
    }

    #[test]
    fn sha1_accepts_compact_json_and_sha256_ids() {
        let sha256 = "a".repeat(64);

        assert_eq!(sha1(&format!(r#"{{"git":{{"sha1":"{sha256}"}}}}"#)), Some(sha256));
    }

    #[test]
    fn sha1_refuses_a_missing_short_or_non_hex_commit() {
        let escape = format!("{{\"git\":{{\"sha1\":\"{}\u{1b}[2J\"}}}}", "a".repeat(36));
        let refused =
            [r#"{"path_in_vcs":""}"#, r#"{"git":{"sha1":""}}"#, r#"{"git":{"sha1":"abc123"}}"#, &escape, r#"{"git":{"sha1":12}}"#];

        assert_eq!(refused.map(sha1), [None, None, None, None, None]);
    }
}
