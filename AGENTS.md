# Working in this repo

`mrk` is a Rust CLI that renders Markdown in the terminal: styled text, syntax-highlighted code, tables, themes, and Mermaid diagrams drawn as images on terminals that speak the kitty graphics protocol (Ghostty, kitty, WezTerm).
The design lives in `specs/`; read `specs/00-vision.md` then `specs/01-architecture.md` before touching code.

## Ground rules

- The specs are the contract. When code and spec disagree, fix one of them in the same commit and say which.
- A spec line that states a number (padding, widths, limits, timings) changes in the same commit as the constant; a spec never claims more than the code does.
- Branches: `<scope>/<kebab-title>` with scope in `feat fix chore refactor docs test ci`; worktrees under `.claude/worktrees/<scope>+<kebab-title>`.
- Commits follow conventional commits; body in two short imperative sentences at most. No co-author trailers.
- `scripts/check` runs what CI runs; a new CI step lands in both.
- A change that deletes files outside its scope says so in the PR; `tests/links.rs` checks that every relative link and heading anchor in the documentation resolves.

## Code

- Rust 2024, `rustfmt.toml` as committed (140 columns, `use_small_heuristics = "Max"`).
- Lints are pedantic (`[lints]` in Cargo.toml): `cargo clippy --all-targets -- -D warnings` must be clean, and `unwrap`/`expect` only live in tests. `unsafe` is forbidden.
- Never allow a correctness lint crate-wide: convert with `try_from`, or put `#[expect(lint, reason = "…")]` on the one expression.
- No comments that say what the code does; a comment must say why, and only when no name can.
- Immutability across function boundaries: never hand a mutable value to a sibling or child; side effects at the edges (`main.rs`, `cli/`, `terminal/`).
- Everything between reading the input and writing the output is pure: `markdown::render` returns a `Document`, it never prints.
- Flat bodies of named steps at one level of abstraction; signatures designed from the call site. No `match () { () if … }`: use `if`/`else` or match a real value.
- One word per concept, the words in `specs/00-vision.md` § Vocabulary.
- One helper per mechanic: wrapping, hard-wrapping, tab stops and width sums live in `text.rs`; never hand-roll them in a module.
- A struct whose fields are all `Copy` derives `Copy`; move owned values instead of cloning them.
- A module owns its dependency: only `markdown/` sees comrak, only `code/` sees syntect, only `mermaid/` sees the mermaid crates, only `raster/` sees resvg, only `terminal/` writes escape sequences.
- A function that builds a terminal row returns it without `\n`; only the writer adds newlines.

## Library boundary

- The library exports what a library user needs and nothing else: arguments, config, network and `process::exit` live in the bin target; test helpers are `#[cfg(test)]`.
- Library errors are typed and never mention the CLI; only the binary uses `anyhow` and adds wording.
- Safety a library user needs (`sanitize`, `Line::printable`, `document::printable`) lives in the core, outside any feature; docs never print `plain()` text.
- Every number from a flag, the environment or the config has a lower and an upper bound, tested in `tests/cli.rs`.

## Tests

```sh
scripts/check                   # everything CI runs
cargo test                      # unit, snapshots, CLI
cargo insta review              # after a deliberate rendering change
cargo run -- tests/fixtures/showcase.md
```

- Every module ships its tests in the same file; rendering tests snapshot `document::plain` (layout) and, where colour matters, `snapshot::ansi` or `terminal::ansi` output.
- Colour assertions compare whole styled lines or ANSI snapshots, never "the first span containing X".
- A test written for a branch proves it reaches that branch (the input really exceeds the limit); a regression test fails on the old code.
- Tests that depend on the machine (installed fonts, timing) are made hermetic or marked `#[ignore = "<reason>"]`; deadlines are injected, never measured.
- Loops over I/O are generic over `Read`/`Write` so they are tested without a terminal.

## Security

`specs/02-security.md` is not optional: Markdown is untrusted input and the terminal is an interpreter.

- Every byte reaching the terminal passes `sanitize`, stderr included: panic reports, error causes, a dependency's messages.
- mrk never touches the network (except `mrk update`, run by the user); anything that installs pins an immutable id (a commit hash) and reports what actually happened, checked after the fact.
- Beyond the file named, stdin and its config, mrk only reads font files, read-only, from the font directories; mermaid-rs-renderer also loads the system fonts and reads and writes a font copy under `$XDG_CACHE_HOME/mmdr`, `~/.cache/mmdr` by default (`specs/02-security.md` rule 3).
- What a dependency reads, writes and runs is what mrk does: check it before a spec claims "never".
- Work is bounded before a dependency is called: an engine whose time grows faster than its input gets a structural limit measured on its worst case, plus a deadline, both written in the spec; measure realistic inputs too, so the limit refuses nothing a person writes.
- A panic a renderer may raise is wrapped in `mermaid::catch::caught`, so the binary's panic hook leaves its report out (`mermaid::is_catching`); the library installs no hook.
- No file is written relative to the current directory; temporary files go to the platform temp folder and are removed.

## Terminal state

- Before raw mode or a terminal query, check that mrk is the tty's foreground process group; read replies until the DA1 marker or the deadline, skip bytes that are not replies, and leave raw mode with `TCSAFLUSH` so unread replies are discarded.
- Every change the pager makes to terminal state (modes, pictures, ids) is undone by `Screen`, only for what was actually sent: a mode is saved and restored, never forced off; a picture is freed by its exact id, never by a range another process may own.
- Signals that end mrk restore the terminal first and are caught only once the screen is taken.
- Cursor moves work when a block is taller than the window (`CSI n A` stops at the top row).
