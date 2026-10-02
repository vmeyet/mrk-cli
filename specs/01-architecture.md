# 01 · Architecture

Package `mrk-cli` (`mrk` is taken on crates.io), library crate `mrk`, binary `mrk`, edition 2024.

## Features

- `cli` (default): the binary and the `cli`, `config`, `terminal` and `update` modules, with `clap`, `clap_complete`, `clap_mangen`, `roff`, `crossterm`, `dirs`, `toml`, `serde`, `shell-words`, `rustix`, `base64` and `png` (decodes mrk's own PNGs for Sixel; already in the tree through tiny-skia).
- Always on, the library core: `document`, `text`, `theme`, `markdown`, `code`, `mermaid`, `raster`, `version`. A consumer depends on mrk with `default-features = false` and gets rendering without any terminal crate; `cargo check --no-default-features` runs in CI and `scripts/check`.

## Pipeline

```
main ─▶ cli::run
          ├─ config::load            ~/.config/mrk/config.toml (optional)
          ├─ terminal::detect        Capabilities { color, graphics, background }
          ├─ theme::Choice           name, or a dark/light pair by background ─▶ Theme
          ├─ read input              file or stdin, capped at 8 MiB
          ├─ markdown::render        &str + &Settings ─▶ Document        (pure)
          │     ├─ code::render      fenced code ─▶ Vec<Line>            (pure)
          │     ├─ mermaid::render   ```mermaid ─▶ Block                 (pure)
          │     └─ raster::line      jumbo title line ─▶ Picture         (pure)
          └─ output                  print, built-in pager, or $MRK_PAGER
                ├─ terminal::write         Document + Capabilities ─▶ stdout
                └─ terminal::pager::run    re-renders through a closure on resize
```

## Crate layout

```
build.rs           embeds the git commit as GIT_HASH for version.rs
assets/fonts/      MrkSans.ttf, rebuilt by scripts/last-resort-font, and the DejaVu license it ships under
src/
  main.rs          calls cli::run, maps errors to `✗ message` and exit code 1, or 2 when a UsageError is in the chain
  lib.rs           module list; `cli`, `config`, `terminal`, `update` behind the `cli` feature
  cli/             clap arguments, precedence flags > env > config, orchestration
    help.rs        the sections after the options (settings, config lookup, pager keys, exit status), as --help text and man roff
    input.rs       file or stdin, capped at 8 MiB, the name the pager shows
    output.rs      Output (print, pager, MRK_PAGER command), Layout: the render closure the pager calls on resize
  config.rs        Config { theme, theme_dark, theme_light, width, images, align, pager, jumbo_title }, strict TOML (unknown keys rejected)
  document.rs      Document, Block, Line, Span, Style, Rgb, Picture, Settings, JumboTitle, CellSize, plain(), highlight()
  text.rs          display_width, cut, wrap
  theme.rs         Theme, Palette, Appearance, SyntaxTheme, built-in presets, Choice, names
  update.rs        `mrk update`: latest release tag from `git ls-remote --tags`, compared with the running version, installed with `cargo install --tag`
  version.rs       the commit the binary was built from, `mrk --version`'s `0.2.0 (a1b2c3d)` label
  markdown/        comrak AST ─▶ Document; one file per block family (inline, list, table, quote, …)
    blocks.rs      SourceBlock, BlockKind: the top-level sections both render and render_blocks are built from
  code/            syntect + two-face: highlight a fenced block into a surface panel
  mermaid/         mermaid-rs-renderer SVG ─▶ raster PNG ─▶ Picture; mermaid-text fallback
  raster/          resvg and its fonts: an SVG ─▶ PNG, a line of text ─▶ Picture (a jumbo title); no other module sees resvg
  terminal/        detect, write (ANSI + OSC 8 + kitty graphics or Sixel), sanitize, colour downgrade
    sixel.rs       PNG ─▶ at most 256 colours ─▶ Sixel bands; pure, no dependency beyond `png`
    pager/         the built-in pager, below
```

## Pager

`mrk -p` on a terminal takes the alternate screen until `q`, even when the document fits: it is meant to be the program another TUI hands the terminal to.
`less` strips kitty graphics and Sixel, so the pager is built in; `$MRK_PAGER` swaps it for a command, split into words with `shell-words` and run without a shell, fed the printed ANSI with pictures off.

- Pure: `page.rs` flattens the Document into rows (a `Line` is one row, a `Picture` spans `rows`), once per render; `state.rs` holds the scroll position, clamped to `rows - height`, the search and the prompt, and changes only through `Pager::apply(Action)`; `keys.rs` maps keys to actions through one binding table that `--help` and the man page also list; `search.rs` finds case-insensitive literal matches over `Line::plain` and paints them with `document::highlight`; `status.rs` lays out the last row; `picture.rs` clips a picture to the rows on screen; `frame.rs` turns all of it into one string per frame.
- Edge: `screen.rs` takes raw mode, the alternate screen, a hidden cursor and no autowrap, and gives them back from `Drop`, which also runs when a panic unwinds out of the pager (release builds unwind); while held, a panic hook only keeps the pager thread's panic report, printed once the terminal is back, so a panic the mermaid renderers catch leaves the pager running; `mod.rs` runs the crossterm event loop and redraws only after an event. A resize that changes the width renders the source again through the caller's closure; a burst of resizes renders once.
- Kitty pictures: each is sent once per render with an image id (`a=t,i=…`), then every frame first removes the placements (`a=d,d=i`, lowercase keeps the data) and places the visible ones at the cursor (`a=p,i=…,p=1,z=…,C=1`), after the indent drawn on their rows.
  A jumbo title's picture sits on `z=-1`, under the text and above cell backgrounds, and its first row holds the title written concealed (`CSI 8m`), in the pager as in printed output, so selection copies the words; the highlight only covers the left half, since the hidden text is one cell per character.
- Double-height lines: a `Block::DoubleHeight` line is two rows, `ESC # 3` then `ESC # 4` before the same line drawn at half the margin, every cell of the row being twice as wide. Erasing a row keeps its size, so on a terminal that draws them every pager row starts with `ESC # 5`. They are not searched in the pager, nor are title pictures. A picture partly off screen is cropped with the source rectangle (`x,y,w,h`, the pixel band behind its visible rows) and drawn into `c×r` visible cells, so it scrolls row by row. On exit every pager id is freed (`a=d,d=R`). The frame is one write inside synchronized output (`CSI ?2026h … l`), so nothing flickers.
- Sixel pictures have no id to place or delete: each is reduced to its palette once per render, and every frame sends the band of pixel rows behind its visible rows again, after the text rows whose erase (`CSI 2K`) wiped last frame's pixels.
  Sixel output is cut to whole six-pixel bands, so a picture never reaches into the row below it, and printed pictures sit between `ESC 7` and `ESC 8`, since terminals leave the cursor in different places after an image.
  Pixels under half opacity are left transparent; Sixel has no partial alpha.
- Input: keys come from the terminal even when the Markdown came from stdin; crossterm's `use-dev-tty` feature reads `/dev/tty` with `poll`/`select`, which kqueue refuses on macOS.
- The mouse is not captured, so text selection keeps working; the pager turns on alternate scroll mode (`?1007`), so terminals that honour it send the wheel as arrow keys and scroll the pager with it.

## Contracts

- `markdown::render(source: &str, settings: &Settings) -> Document`
- `markdown::render_blocks(source: &str, settings: &Settings) -> Vec<SourceBlock>`: the same rendering cut into top-level blocks, each with `first_line`/`last_line` (1-based, inclusive, from comrak's `sourcepos`, trailing blank lines dropped), a `BlockKind` and its `blocks`. A list gives one block per top-level item, nested items inside; quotes and alerts stay whole; the footnote definitions are one last block spanning the earliest to the latest definition. No blank line sits before, after or between blocks. `render` is these blocks stacked a blank line apart (none between items of a tight list), so the two cannot drift.
- `document::highlight(lines: &[Line], ranges: &[Range<usize>], restyle: impl Fn(Style) -> Style) -> Vec<Line>`: ranges are char offsets into the lines' plain text joined with no separator; spans split at range edges and keep their link, text and widths never change. The pager search paints its matches with it.
- `code::render(code: &str, language: Option<&str>, settings: &Settings) -> Vec<Line>` returns the whole panel, every line exactly `settings.width` cells.
  `language` is a language name: `markdown` reads it once from the fence info string, the first word lowercased (`Rust,ignore` and `{.rust}` give `rust`), and the same word picks Mermaid and fills `BlockKind::Code`.
- `mermaid::render(source: &str, settings: &Settings) -> Block`: `Block::Picture` when `settings.cell` is `Some` and the diagram renders, otherwise `Block::Lines` (box-drawing text, or the source in a code panel when even that fails, with a one-line muted note).
- `raster::line(text: &str, color: Rgb, cell: CellSize, rows: u16, max_cols: usize) -> Result<Picture, RasterError>`: the text in bold `font-weight` on a transparent PNG `rows` cells tall and as many whole cells wide as it needs, shrunk to fit `max_cols`; `raster::Svg` parses and draws mermaid's SVG the same way.
- `settings.jumbo_title` (from `terminal::jumbo_title(&Capabilities)`, set only when `--jumbo-title` or `jumbo_title = true` asks): `Picture` turns each line of a top-level H1, wrapped at half the width, into a two-row `Block::Picture` with `concealed_text`, or the plain H1 when `cell` is `None` or a line fails to draw; `DoubleHeight` makes them one `Block::DoubleHeight`. Only a top-level H1 grows: inside a container (`context.nesting > 0`) it stays one row beside the bars.
- `terminal::pager::run(session: &Session, render: impl Fn(u16) -> Rendered)`: `render` lays the source out for a window that many columns wide and returns the Document and its margin.
- `terminal::write(document: &Document, capabilities: &Capabilities, margin: usize, out: &mut impl Write)`: writes a Document with its escape sequences; `margin` columns before every line and picture, from `terminal::margin`, then a picture's indent on each of its rows (centred on a wide terminal, `LEFT_MARGIN` otherwise).
- Escape sequences are produced only inside `terminal/`: `write` for documents, `pager/frame.rs` and `pager/screen.rs` for the pager, `kitty.rs`, `sixel.rs` and `tmux.rs` for pictures, `query.rs` for the capability queries, `error_report` for dimmed error causes.
- `settings.hyperlinks` mirrors `Capabilities::hyperlinks`: when links cannot be clicked, `markdown` adds their target as a span so it wraps and counts toward the width; `terminal::write` still sanitizes it like any span text.
- A `Line` never exceeds `settings.width` cells; the renderer that builds it guarantees it, `text::wrap` helps.
- Every width is `text::display_width`: counted per grapheme (at most two cells), a tab as four, the characters `terminal::sanitize` removes as none.
  Every cut to a width goes through `text::cut`, which never splits a grapheme.
- Blank lines between blocks are explicit empty `Line`s emitted by `markdown`.

## Performance

- syntect's syntax and theme sets load lazily, once, on the first code block (`std::sync::LazyLock`).
- The mermaid engine loads lazily on the first diagram, the font database on the first diagram or title picture; fonts are the label families the SVG asks for, plus a short list of per-script fallbacks (CJK, Arabic, Hebrew, Devanagari, Thai) when a label goes beyond Latin; the whole system set loads only when those fallbacks miss a glyph.
  Every font set ends on `mrk Sans`, an embedded Latin subset of DejaVu Sans (about 65 KiB of binary): usvg closes every font list with the generic `serif`, which points at it, so labels render on a system without fonts.
  Renamed, it never shadows a system family the SVG lists.
- Terminal queries (kitty graphics support, the cell size, background colour) share one round trip ended by a DA1 request, with a 100 ms timeout, and only run when stdout is a tty and the answer is not already known from the environment.
  The DA1 reply lists Sixel as attribute `4`; Sixel draws only when kitty graphics does not.
  Inside tmux (`TMUX` set) the environment cannot vouch for kitty graphics, since it was inherited from whatever terminal started the server; only `LC_TERMINAL=iTerm2` still rules them out.
  The kitty query goes through tmux (`ESC Ptmux; … ESC \`, inner ESCs doubled) and the round waits for its answer after tmux's own DA1, up to the timeout: no answer means passthrough is off or the terminal outside has no kitty graphics, and diagrams stay text.
  mrk never runs `tmux` to read `allow-passthrough`.
  A Sixel in tmux's own DA1 means tmux (3.4+, built with it) draws Sixel itself, so Sixel goes out unwrapped.
  Through tmux, kitty pictures are sent once with a virtual placement (`U=1`) and shown by rows of `U+10EEEE` placeholder cells, coloured with a 256-colour index for the id's low byte and numbered by diacritics (row, column, the id's high byte), so tmux keeps, scrolls and redraws them like text; ids come from a hash of the PNG.
  The pager writes the placeholder cells of the visible rows in place of the picture rows, with nothing to place or delete. The cell size comes from the window's pixel size, or from `CSI 16 t` when the window reports none.
  iTerm2 is known to have no graphics: it answers the kitty graphics query with `OK` but draws nothing.
- DEC double-height lines are known from the environment only, never queried: `XTERM_VERSION`, `KONSOLE_VERSION`, `MLTERM`, `WT_SESSION`, `TERM_PROGRAM=iTerm.app` or `LC_TERMINAL=iTerm2`, with no other `TERM_PROGRAM` set (a terminal started from xterm inherits its variables), on a coloured tty outside any multiplexer, which drops them.
- Release profile:
 `opt-level = "s"`, fat LTO, one codegen unit, stripped.

## Release

- dist (cargo-dist) owns the pipeline: `dist-workspace.toml` is the config, `.github/workflows/release.yml` is generated by `dist generate` and never edited by hand.
- Pushing a `vX.Y.Z` tag that matches the package version builds `aarch64`/`x86_64` for `apple-darwin`, `unknown-linux-gnu` and `unknown-linux-musl` on native runners with the `dist` profile (the release profile), then publishes a GitHub release with a shell installer and a Homebrew formula pushed to `vmeyet/homebrew-tap`.
- Linux ships twice: glibc for Homebrew, and static musl that the shell installer falls back to on Alpine or an old glibc.
- The only C code is oniguruma, built by `cc`; the musl jobs compile it with `musl-gcc` from `musl-tools`, and fonts come from the system at run time with the embedded `mrk Sans` as last resort, so nothing links beyond libc.
