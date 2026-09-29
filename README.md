# mrk

Render Markdown beautifully in the terminal: syntax-highlighted code, tables, alerts, footnotes, themes, and Mermaid diagrams drawn as real images in Ghostty, kitty and WezTerm (box-drawing text elsewhere).

```sh
cargo install --git https://github.com/vmeyet/mrk-cli
mrk update        # install the latest release when it is newer; dependencies stay built in the cache
mrk README.md
cat notes.md | mrk
mrk --theme catppuccin-mocha notes.md
mrk --list-themes
mrk -p notes.md   # read in the pager; diagrams stay images
```

## Options

| Flag | Env | Config key | Default |
|---|---|---|---|
| `--theme NAME` | `MRK_THEME` | `theme` | `mrk-dark` / `mrk-light` from the terminal background |
| `--width N` | `MRK_WIDTH` | `width` | terminal width, capped at 100 |
| `--images auto\|always\|never` | | `images` | `auto`: pictures when the terminal speaks the kitty graphics protocol, not under tmux |
| `--align center\|left` | `MRK_ALIGN` | `align` | `center`: the text column sits in the middle of a wide window; piped output is never centred |
| `--color auto\|always\|never` | `NO_COLOR` | | `auto`: colour on a tty; without colour there are no hyperlinks, so links print their target as ` <url>` |
| `-p`, `--pager` | `MRK_PAGER` | `pager` | off; the built-in pager keeps diagrams as images and stays open until `q`; `MRK_PAGER` pipes into that command instead (diagrams as text); piped output is never paged |
| `--completions SHELL` | | | |

Config lives in `~/.config/mrk/config.toml`:

```toml
theme = "tokyo-night"
width = 90
images = "auto"
align = "left"
pager = true
```

Pager keys: `j`/`k`/arrows scroll a row, `space`/`b` a page, `d`/`u` half a page, `g`/`G` top and bottom, `/` searches, `n`/`N` move between matches, `q` quits.

## Library

Render Markdown into styled lines and pictures from Rust, without the CLI and terminal dependencies:

```toml
mrk = { git = "https://github.com/vmeyet/mrk-cli", tag = "v0.2.0", default-features = false }
```

```rust
use mrk::document::{Block, Settings};
use mrk::{markdown, theme};

let settings = Settings { width: 80, theme: theme::find("tokyo-night").unwrap_or(theme::MRK_DARK), cell: None };
for block in markdown::render_blocks(source, &settings) {
    println!("{:?}, lines {}-{}", block.kind, block.first_line, block.last_line);
    for part in &block.blocks {
        if let Block::Lines(lines) = part {
            lines.iter().for_each(|line| println!("  {}", line.plain()));
        }
    }
}
```

`markdown::render` gives the whole `Document`; `document::highlight` restyles char ranges of a block's text, for search or word-level diffs.
Set `cell` to the terminal cell size in pixels to get Mermaid diagrams as PNG pictures.

## Develop

```sh
scripts/check     # fmt, clippy, tests, cargo-deny, machete
cargo run -- tests/fixtures/showcase.md
```

Design and rules: `AGENTS.md` and `specs/`.
