# 01 · Architecture

Package `mrk-cli` (`mrk` is taken on crates.io), library crate `mrk`, binary `mrk`, edition 2024.

## Features

- `cli` (default): the library's `terminal` module and the binary, whose own `cli`, `config`, `update` and `version` modules are no part of the library, with `anyhow`, `clap`, `clap_complete`, `clap_mangen`, `roff`, `crossterm`, `toml`, `serde`, `shell-words`, `signal-hook` (already in the tree through crossterm), `rustix` (on unix), `tempfile` (`mrk update`'s build folder), `base64` and `png` (decodes mrk's own PNGs for Sixel; already in the tree through tiny-skia).
- Always on, the library core: `document`, `text`, `sanitize`, `theme`, `markdown`, `diff`, `code`, `mermaid`, `raster`. A consumer depends on mrk with `default-features = false` and gets rendering without any terminal crate; `cargo check --no-default-features` runs in CI and `scripts/check`.
  The core returns typed errors (`theme::UnknownTheme`, `raster::RasterError`); the binary adds its own wording.

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
build.rs           embeds the git commit as GIT_HASH for version.rs, only when the package is the root of its git checkout (a vendored copy reports `unknown`)
assets/fonts/      MrkSans.ttf, rebuilt by scripts/last-resort-font, and the DejaVu license it ships under
src/
  main.rs          the binary: declares `cli`, `config`, `update`, `version`, installs the panic hook, calls cli::run, maps errors to `✗ message` and exit code 1, or 2 when a UsageError is in the chain
  lib.rs           the library's module list; `terminal` behind the `cli` feature
  cli/             clap arguments, precedence flags > env > config, orchestration
    environment.rs MRK_THEME, MRK_WIDTH, MRK_ALIGN, read outside clap so an empty one counts as unset and --help shows no value
    help.rs        the sections after the options (settings, config lookup, pager keys, exit status), as --help text and man roff
    input.rs       file or stdin, capped at 8 MiB, the name the pager shows
    output.rs      Output (print, pager, MRK_PAGER command, Ctrl-C and Ctrl-\ left to it), Layout: the render closure the pager calls on resize, each render's diagrams given 10 s
    panic.rs       the panic hook: reports sanitized, quiet for a panic the Mermaid renderers catch
    signals.rs     signals held off while a child (MRK_PAGER, cargo in a process group of its own) runs, so mrk outlives it, ends it and cleans up, then exits 128 + the signal
  config.rs        Config { theme, theme_dark, theme_light, width, images, align, pager, jumbo_title }, strict TOML (unknown keys rejected); the width bounds MIN_WIDTH 20, MAX_WIDTH 1000 and DEFAULT_WIDTH 100
  diff.rs          sections(): two versions of a file rendered with render_sections, paired section by section, changed words as char ranges (similar)
  document.rs      Document, Block, Line, Span, Style, Rgb, Picture, Settings, JumboTitle, CellSize, plain(), highlight(); Line::printable() is the text safe to print
  sanitize.rs      text, lines, link: what may reach a terminal, for the binary and library users alike; the characters removed, which display_width counts as none
  snapshot.rs      tests only: a Document as terminal::ansi writes it, what colour snapshots compare
  text.rs          display_width, cut, wrap (at spaces), hard_wrap (where a line is full, for code and source), expand_tabs
  theme.rs         Theme, Palette, Appearance, SyntaxTheme, built-in presets, Choice, names, UnknownTheme
  update.rs        `mrk update`: the latest release tag and its commit from `git ls-remote --tags`, compared with the running version;
                   `brew update` then `brew upgrade` (`brew reinstall` when forced) when brew installed mrk, checked against `brew list --versions`,
                   else `cargo install --rev <commit> --target-dir <temporary folder>`, the folder removed afterwards, also on Ctrl-C or SIGTERM,
                   then the installed binary's `--version` and the `mrk` the PATH finds checked
  version.rs       the commit the binary was built from, `mrk --version`'s `0.2.0 (a1b2c3d)` label
  markdown/        comrak AST ─▶ Document; one file per block family (inline, list, table, quote, …)
    blocks.rs      Section, SectionKind: the top-level sections both render and render_sections are built from
  code/            syntect + two-face: highlight a fenced block into a surface panel
  mermaid/         mermaid-rs-renderer SVG ─▶ raster PNG ─▶ Picture; mermaid-text fallback
    budget.rs      the size and subgraph limits of both engines, then each engine's own count
    cache.rs       every diagram a document's budget lets through, drawings (or timeouts) by source and palette, texts by width too
    catch.rs       renderer panics caught, `is_catching` telling a panic hook to leave their reports out
    worker.rs      a renderer on the calling thread, or on its own thread given up at the deadline, never started past it
  raster/          resvg and its fonts: an SVG ─▶ PNG, a line of text ─▶ Picture (a jumbo title); no other module sees resvg
  terminal/        detect, write (ANSI + OSC 8 + kitty graphics or Sixel) through sanitize, colour downgrade
    sixel.rs       PNG ─▶ at most 256 colours ─▶ Sixel bands; pure, no dependency beyond `png`
    pager/         the built-in pager, below
```

## Pager

`mrk -p` on a terminal takes the alternate screen until `q`, even when the document fits: it is meant to be the program another TUI hands the terminal to.
`less` strips kitty graphics and Sixel, so the pager is built in; `$MRK_PAGER` swaps it for a command, split into words with `shell-words` and run without a shell, fed the printed ANSI with pictures off.

- Pure: `page.rs` flattens the Document into rows (a `Line` is one row, a `Picture` spans `rows`), once per render; `state.rs` holds the scroll position, clamped to `rows - height`, the search and the prompt, and changes only through `Pager::apply(Action)`; `keys.rs` maps keys to actions through one binding table that `--help` and the man page also list; `search.rs` finds case-insensitive literal matches over `Line::plain`, paints them with `document::highlight` and, after a new width, keeps the current match by number when the count did not change, else takes the one with nearest as many letters and digits before it (which wrapping keeps), the closest number breaking a tie; `status.rs` lays out the last row; `picture.rs` clips a picture to the rows on screen; `frame.rs` turns all of it into one string per frame.
- Edge: `screen.rs` takes raw mode (on `/dev/tty`, left with `TCSAFLUSH`), the alternate screen, a hidden cursor and no autowrap, keeps what frees the pictures it stored, and gives it all back from `Drop`, which also runs when a panic unwinds out of the pager (release builds unwind).
  Every write to the terminal, and the whole give-back (leave sequence, flush, `TCSAFLUSH`), holds one lock, so the terminal is given back between two writes, once, by the pager thread or the signal thread, whichever comes first; the screen is taken under the same lock.
  While held, a panic hook keeps the report of every panic, from any thread, and prints them sanitized once the terminal is back; it leaves out a panic the mermaid renderers catch (`mermaid::is_catching`, which the binary's panic hook reads too), which leaves the pager running.
  `signals.rs` catches `SIGHUP`, `SIGINT`, `SIGQUIT` and `SIGTERM` (`signal-hook`'s signal thread, no `unsafe`), which a parent program closing the pager sends, from just before the screen is taken (one during the first render ends mrk at once): the signal thread gives the terminal back itself, then ends mrk as the signal would have, so a signal while a new width renders ends mrk at once, and every signal after the first finds the terminal already given back.
  When that takes more than 1 s (a terminal that stopped reading holds a write, or the drain of `TCSAFLUSH`), the signal thread forces the terminal back through a tty handle of its own: the leave sequence after an `ST` that ends any sequence left open, written without blocking (a stalled terminal drops it), and the saved modes restored with `TCSANOW`; then mrk ends.
  Once the pager ended, a signal ends mrk at once.
  `mod.rs` runs the crossterm event loop and redraws only after an event. A resize that changes the width renders the source again through the caller's closure; a burst of resizes renders once.
- Kitty pictures: each is sent once per render with an image id (`a=t,i=…`; kitty keeps ids per window, so outside tmux they count up from `0x6d726b00`, "mrk", clear of the small ids other programs pick; through tmux see Performance), then every frame first removes the placements (`a=d,d=i`, lowercase keeps the data) and places the visible ones at the cursor (`a=p,i=…,p=1,z=…,C=1`), after the indent drawn on their rows.
  A jumbo title's picture sits on `z=-1`, under the text and above cell backgrounds, and its first row holds the title written concealed (`CSI 8m`), in the pager as in printed output, so selection copies the words; the highlight only covers the left half, since the hidden text is one cell per character.
- Double-height lines: a `Block::DoubleHeight` line is two rows, `ESC # 3` then `ESC # 4` before the same line drawn at half the margin, every cell of the row being twice as wide. Erasing a row keeps its size, so on a terminal that draws them every pager row starts with `ESC # 5`. They are not searched in the pager, nor are title pictures. A picture partly off screen is cropped with the source rectangle (`x,y,w,h`, the pixel band behind its visible rows) and drawn into `c×r` visible cells, so it scrolls row by row. A new render and the exit free exactly the ids the pager sent (`a=d,d=I`), never a range, which could free another program's pictures, and nothing where no kitty picture was sent. The frame is one write inside synchronized output (`CSI ?2026h … l`), so nothing flickers.
- Sixel pictures have no id to place or delete: each is reduced to its palette once per render, and every frame sends the band of pixel rows behind its visible rows again, after the text rows whose erase (`CSI 2K`) wiped last frame's pixels; a picture that did not move is sent as the last frame encoded it.
  The last six-pixel band of what is sent is padded with transparent rows, so no pixel row goes missing and nothing past the rows sent is painted, and printed pictures sit between `ESC 7` and `ESC 8`, since terminals leave the cursor in different places after an image.
  Pixels under half opacity are left transparent; Sixel has no partial alpha.
- Input: keys come from the terminal even when the Markdown came from stdin; crossterm's `use-dev-tty` feature reads `/dev/tty` with `poll`/`select`, which kqueue refuses on macOS.
  Alt-modified keys do nothing. A terminal reply that came too late reads as Alt and its introducer (`ESC ]`, `P`, `_`, `^`, `X`), then its text: those keys are dropped up to its end (BEL or `ST`), a key no reply holds, or a pause of 20 ms, since a reply arrives in one burst, so the reader's own keys are never held back.
- The mouse is not captured, so text selection keeps working; the pager turns on alternate scroll mode (`?1007`), so terminals that honour it send the wheel as arrow keys and scroll the pager with it.
  Only where the environment names a terminal known to restore a saved mode (`environment::saves_modes`: xterm, Ghostty, foot, outside a multiplexer) does it save the mode first (XTSAVE, `CSI ?1007s`) and restore it on exit (XTRESTORE, `CSI ?1007r`); elsewhere it leaves `?1007` alone, since a terminal that ignores XTRESTORE would keep the mode the pager set and one that drops the `?` would read `CSI 1007 r` as a scroll region. Asking with DECRQM would cost a round trip before the first frame.

## Contracts

- `markdown::render(source: &str, settings: &Settings) -> Document`
- `markdown::render_sections(source: &str, settings: &Settings) -> Vec<Section>`: the same rendering cut into top-level sections, each with `first_line`/`last_line` (1-based, inclusive, from comrak's `sourcepos`, lines ending at `\n`, `\r\n` or a lone `\r` as comrak counts them, trailing blank lines dropped), a `SectionKind` and its `blocks`. A list gives one section per top-level item, nested items inside; quotes and alerts stay whole; the footnote definitions are one last section spanning the earliest to the latest definition. No blank line sits before, after or between sections. A top-level node the enabled extensions never produce renders as its children and is no section. `render` is these sections stacked a blank line apart (none between items of a tight list), so the two cannot drift.
- `diff::sections(old: &str, new: &str, settings: &Settings) -> Vec<SectionChange>`: both sides through `render_sections`, in reading order, each section of a side exactly once.
  Sections are equal when their kind, their lines (every `Lines` and `DoubleHeight` line, line by line) and their pictures' PNG are. A line is compared by the input's text: its frame spans (`Span::is_frame`: bars, borders, rules, markers, padding, the continuation sign, all that mrk draws) count as one space each, and runs of spaces as one, trimmed, except in code, Mermaid, front matter and HTML, whose spacing is the input's; so a renumbered list stays `Same`, and `a+b` against `a + b` or a box character the input wrote is a change. `similar`'s Patience diff gives `Same`, and in each run of differing sections a removed section pairs with the next added section of the same `SectionKind` variant (a heading's level, a task's state, a fence's language may differ) into `Changed`, the others being `Removed` or `Added`.
  A `Changed` pair carries char ranges of its plain text (`Line::plain` of each line, joined), ready for `document::highlight`: a Myers diff over the words and punctuation outside the frame, each run of changed words one range that never crosses a line end or a frame span; none for Mermaid.
  The work is bounded by counts, never by the clock, so the same files always give the same answer (`02-security.md` rule 4): past 10,000 sections, both sides counted, only the equal sections at the start and the end pair as `Same`; a pair over 20,000 words, both sides counted, or over what is left of the 50,000 words one call diffs, is marked whole, every word of each line, and spends nothing, so a smaller pair after it is still diffed.
  It returns ranges, never colours.
- `document::highlight(lines: &[Line], ranges: &[Range<usize>], restyle: impl Fn(Style) -> Style) -> Vec<Line>`: ranges are char offsets into the lines' plain text joined with no separator, in any order; spans split at range edges and keep their link, text and widths never change. The pager search paints its matches with it.
- `code::render(code: &str, language: Option<&str>, settings: &Settings) -> Vec<Line>` returns the whole panel, every line exactly `settings.width` cells, or `document::MIN_WIDTH` when that is narrower.
  `language` is a language name: `markdown` reads it once from the fence info string, the first word lowercased (`Rust,ignore` and `{.rust}` give `rust`), and the same word picks Mermaid and fills `SectionKind::Code`.
- `mermaid::render(source: &str, settings: &Settings) -> Block`: `Block::Picture` when `settings.cell` is `Some` and the diagram renders, otherwise `Block::Lines` (box-drawing text, or the source in a code panel when even that fails, with a one-line muted note).
  Each engine only sees a diagram within its count of `02-security.md` rule 4; over a count, the note names the limit (`mermaid: over 100 edges, too many to lay out`).
  Within a document `markdown` also applies the document's budget of rule 4 (`mermaid: over the 12 s of layout a document's diagrams share`).
  With `settings.diagram_deadline` set (the binary sets it 10 s after a render starts), each engine runs on a thread of its own, a diagram still laying out at that moment says `too slow to lay out` and one not started yet `out of time for the document's diagrams`; without, every diagram lays out on the calling thread.
- `raster::line(text: &str, color: Rgb, cell: CellSize, rows: u16, max_cols: usize) -> Result<Picture, RasterError>`: the text in bold `font-weight` on a transparent PNG `rows` cells tall and as many whole cells wide as it needs, shrunk to fit `max_cols`; the crate's `raster::Svg` parses and draws mermaid's SVG the same way.
- `settings.jumbo_title` (from `terminal::jumbo_title(&Capabilities)`, set only when `--jumbo-title` or `jumbo_title = true` asks): `Picture` turns each line of a top-level H1, wrapped at half the width, into a two-row `Block::Picture` with `concealed_text`, or the plain H1 when `cell` is `None` or a line fails to draw; `DoubleHeight` makes them one `Block::DoubleHeight`. Only a top-level H1 grows: inside a container (`context.nesting > 0`) it stays one row beside the bars.
- `terminal::pager::run(session: &Session, render: impl Fn(u16) -> Rendered)`: `render` lays the source out for a window that many columns wide and returns the Document and its margin.
- `terminal::write(document: &Document, capabilities: &Capabilities, margin: usize, out: &mut impl Write)`: writes a Document with its escape sequences; `margin` columns before every line and picture, from `terminal::margin`, then a picture's indent on each of its rows (centred on a wide terminal, `LEFT_MARGIN` otherwise).
  A picture is drawn after its rows scrolled into view, the cursor sent back up to the first; one taller than the window is drawn in runs one row shorter than the window, each a crop of its PNG (kitty receives the PNG once per run), since the cursor cannot climb above the top row. A Sixel run's last band is padded with transparent rows, so no pixel row goes missing between runs and none is drawn twice.
- Escape sequences are produced only inside `terminal/`: `ansi.rs` for styled lines, `write` for documents, `pager/frame.rs` and `pager/screen.rs` for the pager, `kitty.rs`, `sixel.rs` and `tmux.rs` for pictures, `query.rs` for the capability queries, `error_report` for dimmed error causes.
- `settings.hyperlinks` mirrors `Capabilities::hyperlinks`: when links cannot be clicked, or `sanitize::link` refuses the target (a relative path), `markdown` adds their target as a span so it wraps and counts toward the width; `terminal::write` still sanitizes it like any span text.
- A `Line` never exceeds `settings.width` cells, for any width from `document::MIN_WIDTH` up; the renderer that builds it guarantees it, `text::wrap` and `text::hard_wrap` help.
  `document::MIN_WIDTH` (8) is the narrowest the renderers lay out, enough for a code panel's padding, its continuation marker and a wide grapheme: a narrower width lays out at 8.
  A list item, quote or footnote whose indent would leave less than 12 cells stops indenting: its blocks keep the container's width, a marker opening the first paragraph (or on a line of its own), a quote or alert without its bar.
  Past 32 nested containers, a container renders as its bare text: bounded recursion on hostile input.
- Every width is `text::display_width`: counted per grapheme (at most two cells), a tab up to the next tab stop, the characters `sanitize` removes as none.
  `text::expand_tabs` is the one tab rule: a tab is the spaces up to the next column that is a multiple of four, counted from the start of the line, so `ab\tc` puts `c` in column 4.
  `hard_wrap` (code, source), `wrap_literal` (HTML, front matter: it breaks at spaces but keeps runs of spaces) and `sanitize` expand tabs this way, `sanitize` once the characters it removes are gone; `wrap` breaks at a tab as at a space, except on a background (inline code), where the tab stops on the line it lands on.
  Every cut to a width goes through `text::cut`, which never splits a grapheme.
- Blank lines between blocks are explicit empty `Line`s emitted by `markdown`.

## Performance

- syntect's syntax and theme sets load lazily, once, on the first code block that names a language other than plain text (`text`, `txt`, `plain`, `plaintext`, `none`) (`std::sync::LazyLock`).
- The mermaid engine loads lazily on the first diagram, the font database on the first diagram or title picture; fonts are the label families the SVG asks for, plus a short list of per-script fallbacks (CJK, Arabic, Hebrew, Devanagari, Thai) when a label goes beyond Latin; every font file in the font directories (`02-security.md` rule 3) loads only when those fallbacks miss a glyph.
  What a label needs is read off the text between the SVG's tags, never the diagram source, so a comment in another script loads nothing more.
  Every font set ends on `mrk Sans`, an embedded Latin subset of DejaVu Sans (about 65 KiB of binary): usvg closes every font list with the generic `serif`, which points at it, so labels render on a system without fonts.
  Renamed, it never shadows a system family the SVG lists.
- A diagram's parsed SVG, or its timeout, is kept by source and palette (`mermaid/cache.rs`, the last 2,400, as many as a document's budget lets through): a pager resize renders the document again, and a diagram is only fitted and rasterised again, never parsed or laid out again (the showcase flowchart: 40 ms, then 12 ms); mermaid-text lays out for the width, so it runs again when the width changes, its text for the last width kept, unless it once ran out of time.
- Terminal queries (kitty graphics support, the cell size, background colour) share one round trip ended by a DA1 request, with a 100 ms timeout, and only run when stdout is a tty, mrk is the terminal's foreground job (a background job would be stopped by `SIGTTOU`) and the answer is not already known from the environment.
  The replies are read until the DA1 reply or the timeout, skipping keys typed meanwhile; raw mode ends with `TCSAFLUSH`, which discards the input still unread, so the shell or the pager never reads a reply that arrived during the round as keys.
  Known limit: a reply arriving after the timeout (a slow link such as SSH, or the terminal outside tmux) reaches the shell or the pager as input.
  The DA1 reply lists Sixel as attribute `4`; Sixel draws only when kitty graphics does not.
  Inside tmux (`TMUX` set) the environment cannot vouch for kitty graphics, since it was inherited from whatever terminal started the server; it can still rule them out, since a wrong no only costs pictures; tmux sets its own `TERM_PROGRAM`, so in practice that is `LC_TERMINAL=iTerm2`, and iTerm2 answers the query yet draws nothing.
  The kitty query goes through tmux (`ESC Ptmux; … ESC \`, inner ESCs doubled) and the round waits for its answer after tmux's own DA1, up to the timeout: no answer means passthrough is off or the terminal outside has no kitty graphics, and diagrams stay text.
  mrk never runs `tmux` to read `allow-passthrough`.
  A Sixel in tmux's own DA1 means tmux (3.4+, built with it) draws Sixel itself, so Sixel goes out unwrapped.
  Through tmux, kitty pictures are sent once with a virtual placement (`U=1`) and shown by rows of `U+10EEEE` placeholder cells, coloured with a 256-colour index for the id's low byte and numbered by diacritics (row, column, the id's high byte), so tmux keeps, scrolls and redraws them like text; printed ids come from a hash of the PNG, 16 bits a 256-colour cell carries (the low 24 bits stay under 256).
  Every tmux pane shares the terminal outside, so the pager's ids follow the process instead, the picture's number (up to 256; a picture past it is not drawn) as the high byte.
  With true colour (`COLORTERM=truecolor` or `24bit`, or a terminal known for it) the low 24 bits are the process id with bit 23 set, carried by a true-colour foreground: process ids stay below 2^22 (Linux's limit; macOS and the BSDs stop under 100 000), so two live pagers never share an id, and bit 23 keeps them clear of every printed id.
  Without it, a tmux that downgrades true colour would blank them, so the low byte is the process id modulo 255, plus one, in a 256-colour cell: two pagers in different panes, or a pager and a printed picture, then share an id once in 255, and the later one's picture shows in both.
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
- `mrk update` outside Homebrew builds from source with cargo rather than downloading the release's binary: it trusts the same git host and needs no checksum or signature scheme, at the cost of a few minutes of compiling. It installs the commit the tag pointed at when listed (the peeled `^{}` line for an annotated tag), so a tag moved between listing and fetching cannot change what builds.
