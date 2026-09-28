# 01 · Architecture

Crate `mrk`, binary `mrk`, edition 2024.

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
  lib.rs           module list
  cli/             clap arguments, precedence flags > env > config, orchestration
    input.rs       file or stdin, capped at 8 MiB, the name the pager shows
    output.rs      Output (print, pager, MRK_PAGER command), Layout: the render closure the pager calls on resize
  config.rs        Config { theme, width, images, align, pager }, strict TOML (unknown keys rejected)
  document.rs      Document, Block, Line, Span, Style, Rgb, Picture, Settings, CellSize, plain()
  text.rs          display_width, wrap
  theme.rs         Theme, Palette, Appearance, built-in presets, resolve, names
  markdown/        comrak AST ─▶ Document; one file per block family (inline, list, table, quote, …)
  code/            syntect + two-face: highlight a fenced block into a surface panel
  mermaid/         mermaid-rs-renderer SVG ─▶ resvg PNG ─▶ Picture; mermaid-text fallback
  terminal/        detect, write (ANSI + OSC 8 + kitty graphics), sanitize, colour downgrade
    pager/         the built-in pager, below
```

## Pager

`mrk -p` on a terminal takes the alternate screen until `q`, even when the document fits: it is meant to be the program another TUI hands the terminal to.
`less` strips kitty graphics, so the pager is built in; `$MRK_PAGER` swaps it for a command, split into words with `shell-words` and run without a shell, fed the printed ANSI with pictures off.

- Pure: `page.rs` flattens the Document into rows (a `Line` is one row, a `Picture` spans `rows`), once per render; `state.rs` holds the scroll position, clamped to `rows - height`, the search and the prompt, and changes only through `Pager::apply(Action)`; `keys.rs` maps keys to actions; `search.rs` finds case-insensitive literal matches over `Line::plain` and splits spans to paint them; `status.rs` lays out the last row; `picture.rs` clips a picture to the rows on screen; `frame.rs` turns all of it into one string per frame.
- Edge: `screen.rs` takes raw mode, the alternate screen, a hidden cursor and no autowrap, and gives them back from `Drop` and from a panic hook (release builds abort, so no destructor runs); `mod.rs` runs the crossterm event loop and redraws only after an event. A resize that changes the width renders the source again through the caller's closure; a burst of resizes renders once.
- Pictures: each is sent once per render with an image id (`a=t,i=…`), then every frame first removes the placements (`a=d,d=i`, lowercase keeps the data) and places the visible ones at the cursor (`a=p,i=…,p=1,C=1`). A picture partly off screen is cropped with the source rectangle (`x,y,w,h`, the pixel band behind its visible rows) and drawn into `c×r` visible cells, so it scrolls row by row. On exit every pager id is freed (`a=d,d=R`). The frame is one write inside synchronized output (`CSI ?2026h … l`), so nothing flickers.
- Input: keys come from the terminal even when the Markdown came from stdin; crossterm's `use-dev-tty` feature reads `/dev/tty` with `poll`/`select`, which kqueue refuses on macOS.
- The mouse is not captured, so text selection keeps working; terminals that translate the wheel into arrow keys on the alternate screen (Ghostty, kitty, WezTerm) scroll the pager with it.

## Contracts

- `markdown::render(source: &str, settings: &Settings) -> Document`
- `code::render(code: &str, language: Option<&str>, settings: &Settings) -> Vec<Line>` returns the whole panel, every line exactly `settings.width` cells.
- `mermaid::render(source: &str, settings: &Settings) -> Block`: `Block::Picture` when `settings.cell` is `Some` and the diagram renders, otherwise `Block::Lines` (box-drawing text, or the source in a code panel when even that fails, with a one-line muted note).
- `terminal::pager::run(session: &Session, render: impl Fn(u16) -> Rendered)`: `render` lays the source out for a window that many columns wide and returns the Document and its margin.
- `terminal::write(document: &Document, capabilities: &Capabilities, margin: usize, out: &mut impl Write)`: the only place escape sequences are produced; `margin` columns before every line and picture, from `terminal::margin` (centred on a wide terminal, `LEFT_MARGIN` otherwise).
- A `Line` never exceeds `settings.width` cells; the renderer that builds it guarantees it, `text::wrap` helps.
- Blank lines between blocks are explicit empty `Line`s emitted by `markdown`.

## Performance

- syntect's syntax and theme sets load lazily, once, on the first code block (`std::sync::LazyLock`).
- The mermaid engine and font database load lazily on the first diagram; fonts are the system set filtered to what the SVG asks for.
- Terminal queries (kitty graphics support, background colour) share one round trip ended by a DA1 request, with a 100 ms timeout, and only run when stdout is a tty and the answer is not already known from the environment.
- Release profile: thin LTO, one codegen unit, stripped.
