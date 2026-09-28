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
          └─ terminal::write         Document + Capabilities ─▶ stdout
```

## Crate layout

```
src/
  main.rs          calls cli::run, maps errors to `✗ message` and exit code 1
  lib.rs           module list
  cli.rs           clap arguments, precedence flags > env > config, orchestration
  config.rs        Config { theme, width, images }, strict TOML (unknown keys rejected)
  document.rs      Document, Block, Line, Span, Style, Rgb, Picture, Settings, CellSize, plain()
  text.rs          display_width, wrap
  theme.rs         Theme, Palette, Appearance, built-in presets, resolve, names
  markdown/        comrak AST ─▶ Document; one file per block family (inline, list, table, quote, …)
  code/            syntect + two-face: highlight a fenced block into a surface panel
  mermaid/         mermaid-rs-renderer SVG ─▶ resvg PNG ─▶ Picture; mermaid-text fallback
  terminal/        detect, write (ANSI + OSC 8 + kitty graphics), sanitize, colour downgrade
```

## Contracts

- `markdown::render(source: &str, settings: &Settings) -> Document`
- `code::render(code: &str, language: Option<&str>, settings: &Settings) -> Vec<Line>` returns the whole panel, every line exactly `settings.width` cells.
- `mermaid::render(source: &str, settings: &Settings) -> Block`: `Block::Picture` when `settings.cell` is `Some` and the diagram renders, otherwise `Block::Lines` (box-drawing text, or the source in a code panel when even that fails, with a one-line muted note).
- `terminal::write(document: &Document, capabilities: &Capabilities, margin: usize, out: &mut impl Write)`: the only place escape sequences are produced; `margin` columns before every line and picture, from `terminal::margin` (centred on a wide terminal, `LEFT_MARGIN` otherwise).
- A `Line` never exceeds `settings.width` cells; the renderer that builds it guarantees it, `text::wrap` helps.
- Blank lines between blocks are explicit empty `Line`s emitted by `markdown`.

## Performance

- syntect's syntax and theme sets load lazily, once, on the first code block (`std::sync::LazyLock`).
- The mermaid engine and font database load lazily on the first diagram; fonts are the system set filtered to what the SVG asks for.
- Terminal queries (kitty graphics support, background colour) share one round trip ended by a DA1 request, with a 100 ms timeout, and only run when stdout is a tty and the answer is not already known from the environment.
- Release profile: thin LTO, one codegen unit, stripped.
