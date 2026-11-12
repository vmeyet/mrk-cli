# Changelog

All notable changes to mrk are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and mrk follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.9.0] - 2026-11-12

### Breaking changes for library users

- `Span::link` is an `Option<Arc<str>>`, set only to a target `sanitize::link` accepts, and `Span::linked` takes any `Into<Arc<str>>`.
- `text`, `raster` (with `raster::line` and `RasterError`), `theme::Choice` and `theme::UnknownTheme` are no longer public, nor `Rgb::hex`, `Rgb::mix` and `Line::joined`.
- `terminal::error_report` takes any `std::error::Error`, no longer an `anyhow::Error`.
- `terminal::ColorChoice`, `ImagesMode` and `Align` no longer implement clap's `ValueEnum` nor serde's `Deserialize`.
- `Settings`, `Style`, `Span`, `Picture`, `JumboTitle`, `Theme`, `Palette`, `SyntaxTheme`, `terminal::Capabilities`, `Graphics`, `Protocol`, `ColorDepth` and `Preferences` are `#[non_exhaustive]`: build `Settings` and `Preferences` from `Default`, then set their fields.

### Added

- mrk is on crates.io: `cargo install --locked mrk-cli`, or `mrk-cli = "0.9"` as a library.
- `mrk --version` shows the commit for a crates.io install too.
- Release archives carry a GitHub build attestation, checked with `gh attestation verify`.
- Library: `Palette::colors` lists a palette's colours.
- The reference documents `mrk update` and `--man`.

### Changed

- Code highlighting is bounded: a document highlights about 256 KiB of code, a line over 512 bytes costing more the longer it is; a block past that, or the rest of a block after a line over 11 KB, prints plain with its label kept.
- A code block in a language with no syntax (`output`, a misspelt name) spends none of the highlighting budget.
- `mrk -p notes.md &` stops like `less` until `fg` opens the pager; a background job that cannot stop (it ignores `SIGTTOU`) fails with `not the terminal's foreground job` instead of changing the terminal's modes under the shell.
- Mermaid: a sequence over 200 participants or a git graph over 200 branches shows as source with its note, before either parser reads it.
- Mermaid: a picture also counts pie slices, timeline periods and events, xy chart categories, series and values, state notes and each part of a sequence frame.
- Mermaid: every diagram parsed spends at least 5 ms of the document's 12 s, even one both counts refuse, so a document of many refused diagrams shows the rest as source with the budget note.
- Mermaid: a picture still laying out at the deadline shows as `too slow to lay out`, no longer `out of time for the document's diagrams`, and a pager resize starts neither engine on it again.
- A table drawn as records cuts each header name to one line with an ellipsis, instead of wrapping it in every record.
- `mrk update` through cargo installs under the running mrk's root only when it holds `.crates.toml`, else under cargo's default root, checked writable before building; cargo reads nothing from the terminal.
- `mrk update` through brew runs the installed mrk to check its release and commit, as through cargo.
- `mrk update` also stops cleanly, its build folder removed, on Ctrl-\\ (131) or a closed terminal (129); under `nohup`, a closed terminal leaves the build running.
- The pager restores autowrap and the cursor as they were on terminals that save modes, instead of turning them on.

### Fixed

- A Mermaid diagram whose subgraphs reuse an id as mermaid-text reads it (a bare `subgraph`, an id ending in a carriage return) shows as source instead of overflowing the stack.
- A Mermaid diagram the picture parser panics on (an architecture group opening on `)`) shows as source instead of exiting with 101.
- A signal that stops `mrk update` while its output is closed exits with 128 + the signal, no longer with a panic (101).
- A long link wrapped over many lines holds its target once: a 400 KB URL no longer costs 1.6 GB.
- An emoji split by a character `sanitize` removes (`❤`, U+200B, U+FE0F) counts as the one emoji printed, so a table row stays as wide as its border.
- In the pager, a picture or title picture with no indent starts at the margin, as when printed, and a search match never splits a grapheme.
- A resize during the pager's first render shows in its first frame.
- Library: a diff keeps the spacing of code inside a list item or a quote, and a renumbered item stays the same when its wider marker wraps it again.
- Library: a diff compares sections by a hash first, so a long shared opening costs little.

### Security

- `sanitize` also removes the invisible operators (U+2061–U+2064), the deprecated format characters (U+206A–U+206F), U+180E, U+FFF9–U+FFFB and U+1D173–U+1D17A, which hide inside a name.
- The config and the user font folders are looked for only under an absolute home or `$XDG_CONFIG_HOME`, never relative to the current directory.
- rustix 1.1.6 clears RUSTSEC-2026-0486.

## [0.8.0] - 2026-10-16

### Added

- `--help`, the man page and the reference list the exit codes 128 + signal: 130 or 131 when Ctrl-C or Ctrl-\\ ends `MRK_PAGER`, 130 or 143 when Ctrl-C or `SIGTERM` stops `mrk update`.
- Library: `Settings` has a `Default`.
- Library: `Line::printable` and `document::printable` give text safe to print.
- Library: `Settings::diagram_deadline` gives a document's diagrams a deadline; without it, layout runs on the calling thread with no clock, so the same input gives the same document.
- Library: `Span::is_frame` marks what mrk draws around the input's text: bars, borders, markers, padding.
- Library: `mermaid::is_catching` tells a panic hook to leave out a renderer panic mrk catches; the library installs no hook.

### Changed

- `--width`, `MRK_WIDTH` and `width` take 20 to 1000 columns; an absurd width exits with 2 instead of panicking.
- A config file that exists but cannot be read exits with 2, like one that does not parse.
- `MRK_THEME`, `MRK_WIDTH` or `MRK_ALIGN` set but empty counts as unset.
- A theme named in `theme_dark` or `theme_light` must exist even when `theme` wins; "did you mean" allows one edit per three typed characters, a swap of two neighbours counting as one.
- `mrk --version` shows `unknown` rather than another repository's commit when mrk is built from inside it.
- Ctrl-C and Ctrl-\\ belong to `MRK_PAGER` while it runs; a pager they end makes mrk exit quietly with 130 or 131.
- `mrk update` through brew runs `brew update` first (`HOMEBREW_NO_AUTO_UPDATE` skips it), reinstalls with `--force`, and fails when the tap has not caught up with the release.
- `mrk update` through cargo installs where the running mrk is, builds in a temporary folder removed afterwards, even on Ctrl-C or `SIGTERM`, and checks that the `mrk` the PATH finds is the one installed.
- Mermaid: a flowchart draws as a picture up to 150 links and as text up to 100 edges, `a & b --> c & d` counting four; over a limit, the source shows with a note naming it.
- Mermaid: a document's diagrams share 12 s of layout, charged at the slowest rate measured for what each one counts; a diagram that does not fit what is left shows as source.
- Mermaid: each render gives its diagrams 10 s; a diagram still laying out then shows as source.
- A pager resize lays no picture out again.
- Pager pictures through tmux get collision-free ids with true colour; without it two pagers may share one.

**Breaking** for library users:

- `mrk::terminal::sanitize` is `mrk::sanitize`, part of the core without the `cli` feature.
- `mrk::cli`, `mrk::config`, `mrk::update` and `mrk::version` are gone, with `theme::test_settings`, `theme::DEFAULT_WIDTH`, `text::TAB`, `text::WIDEST_GRAPHEME` and `text::is_forbidden`.
- `markdown::render_blocks`, `SourceBlock`, `BlockKind`, `diff::blocks` and `BlockChange` are `render_sections`, `Section`, `SectionKind`, `diff::sections` and `SectionChange`.
- `theme::Choice::new` fails with a typed `theme::UnknownTheme`.
- `Settings::diagram_deadline` and `Span::is_frame` are new fields: a struct literal sets them or ends with `..Default::default()`.
- `mermaid::DiagramError`, `mermaid::Load`, `raster::Svg`, `raster::FONT_FAMILY` and `raster::MAX_SIDE_PX` are no longer public.
- The enums likely to grow are `#[non_exhaustive]`.

### Fixed

- Ordered tasks show their number before `✔` or `○`.
- `www.` autolinks no longer repeat their target.
- A link whose text is its target no longer repeats it as ` <target>`.
- A relative link (`[guide](docs/guide.md)`) shows its target, muted, even when hyperlinks are on, since no terminal can open it; a `#fragment` never does, nor an image when hyperlinks are on.
- A table with only a header row shows its names when drawn as records, and narrow nesting keeps its markers.
- Front matter keys are no longer bold, and front matter and HTML blocks keep their runs of spaces (`name:    mrk`), still wrapping at spaces.
- Widths count a grapheme as printed, so tag characters no longer shrink a table cell.
- A tab in inline code stops on the column it lands on, and `sanitize::text` places a tab after a removed newline on the joined line.
- Terminal queries are skipped when mrk runs in the background, so `mrk file &` no longer stops, and keys typed meanwhile no longer hide the replies.
- Alt-modified keys and terminal replies that arrive late no longer act as pager keys; alternate scroll is only touched on terminals that restore it.
- The pager gives the terminal back at once on `SIGTERM`, `SIGHUP`, `SIGINT` or `SIGQUIT`, even while it renders a new width, and a signal after `q` is no longer swallowed.
- A picture taller than the window is drawn in runs, so it lands under its rows.
- In the pager, the current search match stays on the same words after a resize.
- Pager pictures through tmux keep ids of their own across panes, the pager frees only the pictures it sent, and a Sixel picture no longer loses its bottom pixel rows.
- A panic on any thread while the pager runs is printed, sanitized, once the terminal is back.
- An error cause over several lines keeps its lines, indented.
- Diagram labels in Latin no longer load every system font when no label family is installed; `Arial Bold.ttf` and the like count as their family.
- Library: a diff marks changed words line by line, never the bars, borders, markers or `↪` mrk draws, keeps a renumbered list item the same and sees a change of spacing inside a line.
- Library: a diff is bounded by counts: a pair over 20,000 words or over what is left of the 50,000 words a diff compares, and a diff over 10,000 sections, are compared coarsely.
- Library: source lines of `render_sections` are right for files with CR-only line endings.
- The documentation examples are back, and every relative link and anchor in the documentation is checked.

### Security

- Diagram fonts are read only from the font directories, and only font files.
- `mrk update` installs the commit the release tag pointed at when listed, so a tag moved meanwhile cannot change what builds.
- A `file:` link to another host (`file://host/share`) is no longer clickable, since opening it can send the user's credentials to that host.
- `sanitize` also removes the word joiner, the byte order mark and tag characters, which hide inside a name, and writes the line and paragraph separators as a space.
- Panic reports are sanitized like every other stderr line, and `--help` no longer echoes environment values.
- CI pins every action to a commit, runs with read-only tokens, and builds the static Linux release on every pull request.

## [0.7.0] - 2026-10-06

### Added

- Library: `diff::blocks` pairs the blocks of two versions of a Markdown file and marks the changed words.

## [0.6.0] - 2026-10-02

### Added

- `theme_dark` and `theme_light` in the config pick the theme after the terminal background when no `theme` is named.

### Changed

- `--list-themes` groups the themes into Dark and Light and marks the group matching the terminal; piped output stays a flat list.
- Inline code is quieter: its text leans 20% toward the code colour and its background 5%, in every theme.
- Library: `theme::Choice` replaces `theme::resolve`, and `Rgb` has a `mix` method.

## [0.5.0] - 2026-10-02

### Added

- `--jumbo-title`, or `jumbo_title = true` in the config, draws level-1 headings two rows tall: as a picture over the selectable title text on kitty, Ghostty and WezTerm, as double-height text on xterm, Konsole, Windows Terminal, mlterm and iTerm2.

### Changed

- Library: `Settings` has a `jumbo_title` field, `Picture` a `concealed_text` field, and `Block` a `DoubleHeight` variant.

## [0.4.1] - 2026-10-01

### Fixed

- In the pager, `n` and `N` light up only the current match, not the matches at the same columns on other lines.

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

[0.9.0]: https://github.com/vmeyet/mrk-cli/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/vmeyet/mrk-cli/compare/v0.7.0...v0.8.0
[0.7.0]: https://github.com/vmeyet/mrk-cli/compare/v0.6.0...v0.7.0
[0.6.0]: https://github.com/vmeyet/mrk-cli/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/vmeyet/mrk-cli/compare/v0.4.1...v0.5.0
[0.4.1]: https://github.com/vmeyet/mrk-cli/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/vmeyet/mrk-cli/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/vmeyet/mrk-cli/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/vmeyet/mrk-cli/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/vmeyet/mrk-cli/releases/tag/v0.2.0
