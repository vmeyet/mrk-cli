# 01 · Architecture

Crate `mrk`, binary `mrk`, edition 2024.

## Features

- `cli` (default): the binary and the `cli`, `config`, `terminal` and `update` modules, with `clap`, `clap_complete`, `crossterm`, `dirs`, `toml`, `serde`, `shell-words`, `rustix` and `base64`.
- Always on, the library core: `document`, `text`, `theme`, `markdown`, `code`, `mermaid`, `version`. A consumer depends on mrk with `default-features = false` and gets rendering without any terminal crate; `cargo check --no-default-features` runs in CI and `scripts/check`.

## Pipeline

```
main ─▶ cli::run
          ├─ config::load            ~/.config/mrk/config.toml (optional)
          ├─ terminal::detect        Capabilities { color, cell size, images, background }
          ├─ theme::resolve          name or appearance ─▶ Theme
          ├─ read input              file or stdin, capped at 8 MiB
          ├─ markdown::render        &str + &Settings ─▶ Document        (pure)
          │     ├─ code::render      fenced code ─▶ Vec<Line>            (pure)
          │     └─ mermaid::render   ```mermaid ─▶ Block                 (pure)
          └─ output                  print, built-in pager, or $MRK_PAGER
                ├─ terminal::write         Document + Capabilities ─▶ stdout
                └─ terminal::pager::run    re-renders through a closure on resize
```

## Crate layout

```
src/
  main.rs          calls cli::run, maps errors to `✗ message` and exit code 1
  lib.rs           module list; `cli`, `config`, `terminal`, `update` behind the `cli` feature
  cli/             clap arguments, precedence flags > env > config, orchestration
    input.rs       file or stdin, capped at 8 MiB, the name the pager shows
    output.rs      Output (print, pager, MRK_PAGER command), Layout: the render closure the pager calls on resize
  config.rs        Config { theme, width, images, align, pager }, strict TOML (unknown keys rejected)
  document.rs      Document, Block, Line, Span, Style, Rgb, Picture, Settings, CellSize, plain(), highlight()
  text.rs          display_width, wrap
  theme.rs         Theme, Palette, Appearance, SyntaxTheme, built-in presets, resolve, names
  markdown/        comrak AST ─▶ Document; one file per block family (inline, list, table, quote, …)
    blocks.rs      SourceBlock, BlockKind: the top-level sections both render and render_blocks are built from
  code/            syntect + two-face: highlight a fenced block into a surface panel
  mermaid/         mermaid-rs-renderer SVG ─▶ resvg PNG ─▶ Picture; mermaid-text fallback
  terminal/        detect, write (ANSI + OSC 8 + kitty graphics), sanitize, colour downgrade
    pager/         the built-in pager, below
```

## Pager

`mrk -p` on a terminal takes the alternate screen until `q`, even when the document fits: it is meant to be the program another TUI hands the terminal to.
`less` strips kitty graphics, so the pager is built in; `$MRK_PAGER` swaps it for a command, split into words with `shell-words` and run without a shell, fed the printed ANSI with pictures off.

- Pure: `page.rs` flattens the Document into rows (a `Line` is one row, a `Picture` spans `rows`), once per render; `state.rs` holds the scroll position, clamped to `rows - height`, the search and the prompt, and changes only through `Pager::apply(Action)`; `keys.rs` maps keys to actions; `search.rs` finds case-insensitive literal matches over `Line::plain` and paints them with `document::highlight`; `status.rs` lays out the last row; `picture.rs` clips a picture to the rows on screen; `frame.rs` turns all of it into one string per frame.
- Edge: `screen.rs` takes raw mode, the alternate screen, a hidden cursor and no autowrap, and gives them back from `Drop`, which also runs when a panic unwinds out of the pager (release builds unwind); while held, a panic hook only keeps the pager thread's panic report, printed once the terminal is back, so a panic the mermaid renderers catch leaves the pager running; `mod.rs` runs the crossterm event loop and redraws only after an event. A resize that changes the width renders the source again through the caller's closure; a burst of resizes renders once.
- Pictures: each is sent once per render with an image id (`a=t,i=…`), then every frame first removes the placements (`a=d,d=i`, lowercase keeps the data) and places the visible ones at the cursor (`a=p,i=…,p=1,C=1`). A picture partly off screen is cropped with the source rectangle (`x,y,w,h`, the pixel band behind its visible rows) and drawn into `c×r` visible cells, so it scrolls row by row. On exit every pager id is freed (`a=d,d=R`). The frame is one write inside synchronized output (`CSI ?2026h … l`), so nothing flickers.
- Input: keys come from the terminal even when the Markdown came from stdin; crossterm's `use-dev-tty` feature reads `/dev/tty` with `poll`/`select`, which kqueue refuses on macOS.
- The mouse is not captured, so text selection keeps working; terminals that translate the wheel into arrow keys on the alternate screen (Ghostty, kitty, WezTerm) scroll the pager with it.

## Contracts

- `markdown::render(source: &str, settings: &Settings) -> Document`
- `markdown::render_blocks(source: &str, settings: &Settings) -> Vec<SourceBlock>`: the same rendering cut into top-level blocks, each with `first_line`/`last_line` (1-based, inclusive, from comrak's `sourcepos`, trailing blank lines dropped), a `BlockKind` and its `blocks`. A list gives one block per top-level item, nested items inside; quotes and alerts stay whole; the footnote definitions are one last block spanning the earliest to the latest definition. No blank line sits before, after or between blocks. `render` is these blocks stacked a blank line apart (none between items of a tight list), so the two cannot drift.
- `document::highlight(lines: &[Line], ranges: &[Range<usize>], restyle: impl Fn(Style) -> Style) -> Vec<Line>`: ranges are char offsets into the lines' plain text joined with no separator; spans split at range edges and keep their link, text and widths never change. The pager search paints its matches with it.
- `code::render(code: &str, language: Option<&str>, settings: &Settings) -> Vec<Line>` returns the whole panel, every line exactly `settings.width` cells.
- `mermaid::render(source: &str, settings: &Settings) -> Block`: `Block::Picture` when `settings.cell` is `Some` and the diagram renders, otherwise `Block::Lines` (box-drawing text, or the source in a code panel when even that fails, with a one-line muted note).
- `terminal::pager::run(session: &Session, render: impl Fn(u16) -> Rendered)`: `render` lays the source out for a window that many columns wide and returns the Document and its margin.
- `terminal::write(document: &Document, capabilities: &Capabilities, margin: usize, out: &mut impl Write)`: the only place escape sequences are produced; `margin` columns before every line and picture, from `terminal::margin` (centred on a wide terminal, `LEFT_MARGIN` otherwise).
- A `Line` never exceeds `settings.width` cells; the renderer that builds it guarantees it, `text::wrap` helps.
- Blank lines between blocks are explicit empty `Line`s emitted by `markdown`.

## Performance

- syntect's syntax and theme sets load lazily, once, on the first code block (`std::sync::LazyLock`).
- The mermaid engine and font database load lazily on the first diagram; fonts are the label families the SVG asks for, plus a short list of per-script fallbacks (CJK, Arabic, Hebrew, Devanagari, Thai) when a label goes beyond Latin; the whole system set loads only when those fallbacks miss a glyph.
- Terminal queries (kitty graphics support, background colour) share one round trip ended by a DA1 request, with a 100 ms timeout, and only run when stdout is a tty and the answer is not already known from the environment.
- Release profile: thin LTO, one codegen unit, stripped.
