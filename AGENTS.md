# Working in this repo

`mrk` is a Rust CLI that renders Markdown in the terminal: styled text, syntax-highlighted code, tables, themes, and Mermaid diagrams drawn as images on terminals that speak the kitty graphics protocol (Ghostty, kitty, WezTerm).
The design lives in `specs/`; read `specs/00-vision.md` then `specs/01-architecture.md` before touching code.

## Ground rules

- The specs are the contract. When code and spec disagree, fix one of them in the same commit and say which.
- Branches: `<scope>/<kebab-title>` with scope in `feat fix chore refactor docs test ci`; worktrees under `.claude/worktrees/<scope>+<kebab-title>`.
- Commits follow conventional commits; body in two short imperative sentences at most. No co-author trailers.

## Code

- Rust 2024, `rustfmt.toml` as committed (140 columns, `use_small_heuristics = "Max"`).
- Lints are pedantic (`[lints]` in Cargo.toml): `cargo clippy --all-targets -- -D warnings` must be clean, and `unwrap`/`expect` only live in tests. `unsafe` is forbidden.
- No comments that say what the code does; a comment must say why, and only when no name can.
- Immutability across function boundaries: never hand a mutable value to a sibling or child; side effects at the edges (`main.rs`, `cli.rs`, `terminal/`).
- Everything between reading the input and writing the output is pure: `markdown::render` returns a `Document`, it never prints.
- Flat bodies of named steps at one level of abstraction; signatures designed from the call site.
- One word per concept, the words in `specs/00-vision.md` § Vocabulary.
- A module owns its dependency: only `markdown/` sees comrak, only `code/` sees syntect, only `mermaid/` sees the mermaid crates, only `raster/` sees resvg, only `terminal/` writes escape sequences.

## Tests

```sh
cargo test                      # unit, snapshots, CLI
cargo insta review              # after a deliberate rendering change
cargo run -- tests/fixtures/showcase.md
```

Every module ships its tests in the same file; rendering tests snapshot `document::plain` (layout) and, where colour matters, `terminal::ansi` output.

## Security

`specs/02-security.md` is not optional: Markdown is untrusted input and the terminal is an interpreter.
Every byte reaching the terminal passes `terminal::sanitize`; mrk never touches the network (except `mrk update`, run by the user) and never reads a file the user did not name.
