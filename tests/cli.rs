#![cfg(feature = "cli")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use assert_cmd::Command;
use predicates::prelude::*;

const EIGHT_MIB: usize = 8 * 1024 * 1024;

fn mrk() -> Command {
    let mut command = Command::cargo_bin("mrk").unwrap();
    command
        .env_remove("MRK_THEME")
        .env_remove("MRK_WIDTH")
        .env("XDG_CONFIG_HOME", env!("CARGO_TARGET_TMPDIR"))
        .env("HOME", env!("CARGO_TARGET_TMPDIR"));
    command
}

#[test]
fn list_themes_prints_bare_names_when_piped() {
    mrk()
        .arg("--list-themes")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("mrk-dark\nmrk-light\n"))
        .stdout(predicate::str::contains('\x1b').not());
}

#[test]
fn list_themes_shows_swatches_when_colour_is_forced() {
    mrk()
        .args(["--list-themes", "--color", "always"])
        .assert()
        .success()
        .stdout(predicate::str::contains("██"))
        .stdout(predicate::str::contains("\x1b[38;"));
}

#[test]
fn completions_print_a_script() {
    mrk().args(["--completions", "zsh"]).assert().success().stdout(predicate::str::contains("#compdef mrk"));
}

#[test]
fn the_man_page_is_roff_with_the_pager_keys() {
    mrk()
        .arg("--man")
        .assert()
        .success()
        .stdout(predicate::str::starts_with(".ie \\n(.g .ds Aq"))
        .stdout(predicate::str::contains(".SH \"PAGER KEYS\""));
}

#[test]
fn the_man_flag_stands_alone() {
    mrk().args(["--man", "a.md"]).assert().code(2);
}

#[test]
fn long_help_names_the_config_file_and_the_pager_keys() {
    mrk().arg("--help").assert().success().stdout(
        predicate::str::contains("~/.config/mrk/config.toml")
            .and(predicate::str::contains("~/Library/Application Support/mrk/config.toml"))
            .and(predicate::str::contains("Flags win over the environment"))
            .and(predicate::str::contains("q, Esc, Ctrl-c"))
            .and(predicate::str::contains("does not turn paging on")),
    );
}

#[test]
fn a_missing_file_is_an_error_naming_it() {
    mrk().arg("does-not-exist.md").assert().code(1).stderr(predicate::str::contains("✗ reading does-not-exist.md"));
}

#[test]
fn oversized_input_is_refused() {
    mrk().write_stdin(vec![b'a'; EIGHT_MIB + 1]).assert().code(1).stderr(predicate::str::contains("larger than 8 MiB"));
}

#[test]
fn an_unknown_theme_exits_with_2() {
    mrk().args(["--theme", "nope", "-"]).write_stdin("# hi").assert().code(2).stderr(predicate::str::contains("unknown theme \"nope\""));
}

#[test]
fn an_unknown_theme_from_the_environment_exits_with_2() {
    mrk().env("MRK_THEME", "nope").write_stdin("# hi").assert().code(2).stderr(predicate::str::contains("unknown theme"));
}

#[test]
fn a_far_theme_name_gets_no_suggestion() {
    mrk().args(["--theme", "bad", "-"]).write_stdin("# hi").assert().code(2).stderr(predicate::str::contains("did you mean").not());
}

#[test]
fn a_bad_flag_exits_with_2() {
    mrk().args(["--width", "5"]).assert().code(2);
}

#[test]
fn a_malformed_config_exits_with_2_naming_it() {
    let home = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("malformed-config");
    std::fs::create_dir_all(home.join("mrk")).unwrap();
    std::fs::write(home.join("mrk/config.toml"), "colour = \"always\"\n").unwrap();

    mrk().env("XDG_CONFIG_HOME", &home).write_stdin("# hi").assert().code(2).stderr(
        predicate::str::contains("invalid config").and(predicate::str::contains("config.toml")).and(predicate::str::contains("line 1")),
    );
}

#[test]

fn a_file_renders_as_plain_text_when_piped() {
    mrk()
        .arg("tests/fixtures/showcase.md")
        .assert()
        .success()
        .stdout(predicate::str::is_empty().not())
        .stdout(predicate::str::contains('\x1b').not());
}

#[test]

fn stdin_renders_with_a_dash_or_nothing() {
    mrk().arg("-").write_stdin("hello").assert().success().stdout(predicate::str::contains("  hello"));
    mrk().write_stdin("hello").assert().success().stdout(predicate::str::contains("  hello"));
}

#[test]

fn forced_colour_carries_hyperlinks() {
    mrk()
        .args(["--color", "always", "--theme", "mrk-dark"])
        .write_stdin("[docs](https://example.com)")
        .assert()
        .success()
        .stdout(predicate::str::contains("\x1b]8;;https://example.com\x1b\\"));
}

#[test]
fn piped_links_print_their_target() {
    mrk().write_stdin("[docs](https://example.com)").assert().success().stdout(predicate::str::contains("docs <https://example.com>"));
}

#[test]

fn hostile_input_never_reaches_the_terminal() {
    let hostile = "# \x1b]0;title\x07 \x1b]52;c;AAAA\x07\n\n\x1b[2J text \u{9b}31m [x](javascript:alert(1))\n\n```\n\x1b[31m\n```\n";

    mrk().args(["--color", "always"]).write_stdin(hostile).assert().success().stdout(
        predicate::str::contains("\x1b]0;")
            .not()
            .and(predicate::str::contains("\x1b]52").not())
            .and(predicate::str::contains("\x1b[2J").not())
            .and(predicate::str::contains("javascript").not())
            .and(predicate::str::contains('\u{9b}').not()),
    );
}

#[test]

fn a_closed_pipe_exits_quietly() {
    let big = "line\n\n".repeat(200_000);
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_mrk"))
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.take().expect("stdin").write_all(big.as_bytes())?;
            drop(child.stdout.take());
            child.wait_with_output()
        })
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn the_pager_prints_as_usual_when_stdout_is_not_a_terminal() {
    let plain = mrk().write_stdin("# Title\n\ntext\n").assert().success().get_output().stdout.clone();

    mrk().arg("-p").env("MRK_PAGER", "false").write_stdin("# Title\n\ntext\n").assert().success().stdout(plain);
}

const HOSTILE: &str = "\x1b]0;pwned\x07\u{9b}31m";

fn assert_clean(stderr: &[u8]) {
    let text = String::from_utf8_lossy(stderr);
    assert!(!text.is_empty() && !text.chars().any(|character| character.is_control() && character != '\n'), "{text:?}");
}

#[test]
fn an_escape_in_a_file_name_never_reaches_stderr() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join(format!("{HOSTILE}.md"));
    std::fs::write(&file, b"caf\xe9").unwrap();

    let stderr = mrk().arg(&file).assert().success().get_output().stderr.clone();

    assert_clean(&stderr);
    assert!(String::from_utf8_lossy(&stderr).contains("]0;pwned31m.md is not valid UTF-8"));
}

#[test]
fn an_escape_in_a_missing_file_name_never_reaches_stderr() {
    assert_clean(&mrk().arg(format!("{HOSTILE}.md")).assert().code(1).get_output().stderr);
}

#[test]
fn an_escape_in_a_flag_never_reaches_stderr() {
    assert_clean(&mrk().arg(format!("--{HOSTILE}")).assert().code(2).get_output().stderr);
}

#[test]
fn an_escape_in_an_environment_value_never_reaches_stderr() {
    for variable in ["MRK_WIDTH", "MRK_ALIGN"] {
        assert_clean(&mrk().env(variable, HOSTILE).write_stdin("# hi").assert().code(2).get_output().stderr);
    }
}
