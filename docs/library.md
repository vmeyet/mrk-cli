# Library

mrk's renderer is a Rust library without any terminal dependency.
It turns Markdown into styled lines and PNG pictures; printing them is up to you.

The package is `mrk-cli` and the crate is `mrk`.
Turn off the default `cli` feature to leave out the binary and its dependencies:

```toml
mrk-cli = { git = "https://github.com/vmeyet/mrk-cli", default-features = false }
```

```rust
use mrk::document::{Block, Settings};
use mrk::{markdown, theme};

let theme = theme::find("tokyo-night").unwrap_or(theme::MRK_DARK);
let settings = Settings { width: 80, theme, cell: None, hyperlinks: false, jumbo_title: None };
for block in markdown::render_blocks(source, &settings) {
    println!("{:?}, lines {}-{}", block.kind, block.first_line, block.last_line);
    for part in &block.blocks {
        if let Block::Lines(lines) = part {
            lines.iter().for_each(|line| println!("  {}", line.printable()));
        }
    }
}
```

## Printing safely

Markdown is untrusted input and a terminal interprets escape sequences: a README can hold `\x1b]52;c;…\x07`, which writes the clipboard, or a `javascript:` link.
A `Document` keeps the text as the input wrote it, so print it through `mrk::sanitize`, which every build includes:

- `Line::printable` gives a line's text with every control, DEL and bidi override removed and tabs expanded; `Line::plain` keeps them, for searching and comparing only.
- `sanitize::text` does the same for any other string you print: a span's text, a picture's `alt` or `concealed_text`.
- `sanitize::link` gives the target to put in an OSC 8 hyperlink, or `None` when the link must not be clickable (`javascript:`, a `file:` link to another host, over 2,048 bytes).

- `markdown::render` gives the whole `Document`.
- `markdown::render_blocks` cuts it into top-level blocks, each tagged with the source lines it came from.
- `document::highlight` restyles character ranges of a block's text, for search or word-level diffs.
- `diff::blocks` pairs the blocks of two versions of a file as `Same`, `Added`, `Removed` or `Changed`, a changed pair with the char ranges of its changed words on each side, for `highlight`.
- Set `cell` to the terminal cell size in pixels to get Mermaid diagrams as PNG pictures instead of text.
- Set `jumbo_title` to draw level-1 headings two rows tall: `Picture` (with `cell` set) gives a picture per line, its text in `concealed_text`; `DoubleHeight` gives `Block::DoubleHeight` lines, for terminals that draw DEC double-height lines.

The contracts behind these functions are in [`specs/01-architecture.md`](../specs/01-architecture.md).
