# Changelog

All notable changes to mrk are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and mrk follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.4.0] - 2026-10-01

### Added

- Mermaid diagrams drawn as Sixel images on terminals without kitty graphics that list Sixel in their DA1 reply, in the pager too.
- Mermaid diagrams drawn as images inside tmux: kitty graphics through passthrough (`set -g allow-passthrough on`) with Unicode placeholders, Sixel when tmux draws it.
- Static Linux binaries (musl, x86_64 and aarch64), which the shell installer picks on Alpine or an old glibc.
- An embedded last-resort font, so diagram labels render on a system without fonts.

## [0.3.1] - 2026-09-30

### Fixed

- iTerm2 shows Mermaid diagrams as text instead of blank space: it answers the kitty graphics query but draws no picture.
- The pager scrolls with the mouse wheel on terminals that honour alternate scroll mode, iTerm2 included, and text selection keeps working.

## [0.3.0] - 2026-09-30

### Changed

- `mrk update` installs the latest release tag instead of the newest commit on `main`, and does nothing when the running version is already that release.
- The package is named `mrk-cli`; the binary and the library crate stay `mrk`.
- A bad theme, config file or `MRK_PAGER` value exits with 2, like any other bad value.
- Release builds are smaller (`opt-level = "s"`, fat LTO) and highlight code about 3x faster with onig.
- Code panel labels are lowercase.

### Added

- Prebuilt binaries for macOS and Linux, with a shell installer and a Homebrew formula.
- `--help` lists the config file lookup, setting order, pager keys and exit codes; `mrk --man` prints a man page.
- Without hyperlinks, links and images print their target as ` <url>`.
- CI runs the tests on Linux as well as macOS, and checks that the minimum supported Rust version (1.92) builds.
- Package metadata (keywords, categories, homepage) and a package that ships only the sources.

### Fixed

- A Mermaid flowchart that reuses or deeply nests subgraphs no longer crashes mrk.
- A panic caught inside a diagram renderer no longer leaves the pager without its terminal.
- Diagrams inside list items and quotes keep their marker, bar and indent.
- Fences such as ```` ```Mermaid ```` or ```` ```mermaid,ignore ```` render as diagrams.
- Emoji sequences and escape bytes no longer push a line past the width.
- File names and usage errors are sanitized before reaching stderr.
- Diagram labels in CJK and other non-Latin scripts load their fonts in milliseconds instead of scanning every system font.

## [0.2.0] - 2026-09-28

### Added

- Render GFM Markdown in the terminal: headings, lists, tasks, tables, quotes, alerts, footnotes, front matter, links as OSC 8 hyperlinks.
- Syntax-highlighted code blocks in a themed panel.
- Built-in themes, light or dark picked from the terminal background.
- Mermaid diagrams drawn as images on terminals that speak the kitty graphics protocol, box-drawing text elsewhere.
- Centre the text column in wide terminals.
- `mrk update`, which rebuilds mrk with a build folder kept between updates.
- A built-in pager (`-p`) that keeps diagrams as images, with search; `MRK_PAGER` pipes into another command instead.
- `markdown::render_blocks`, the rendering cut into top-level blocks tagged with their source lines, and `document::highlight` to restyle ranges of a block.
- A `cli` feature, on by default, so the library renders without any terminal dependency.

### Security

- Every byte written to the terminal passes one sanitizer; links are filtered by scheme and length; input and diagrams are size-capped.

[0.4.0]: https://github.com/vmeyet/mrk-cli/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/vmeyet/mrk-cli/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/vmeyet/mrk-cli/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/vmeyet/mrk-cli/releases/tag/v0.2.0
