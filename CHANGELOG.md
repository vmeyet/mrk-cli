# Changelog

All notable changes to mrk are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and mrk follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- `mrk update` installs the latest release tag instead of the newest commit on `main`, and does nothing when the running version is already that release.

### Added

- CI runs the tests on Linux as well as macOS, and checks that the minimum supported Rust version (1.92) builds.
- Package metadata (keywords, categories, homepage) and a package that ships only the sources.

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

[Unreleased]: https://github.com/vmeyet/mrk-cli/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/vmeyet/mrk-cli/releases/tag/v0.2.0
