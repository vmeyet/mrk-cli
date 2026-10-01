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

let settings = Settings { width: 80, theme: theme::find("tokyo-night").unwrap_or(theme::MRK_DARK), cell: None, hyperlinks: false };
for block in markdown::render_blocks(source, &settings) {
    println!("{:?}, lines {}-{}", block.kind, block.first_line, block.last_line);
    for part in &block.blocks {
        if let Block::Lines(lines) = part {
            lines.iter().for_each(|line| println!("  {}", line.plain()));
        }
    }
}
```

- `markdown::render` gives the whole `Document`.
- `markdown::render_blocks` cuts it into top-level blocks, each tagged with the source lines it came from.
- `document::highlight` restyles character ranges of a block's text, for search or word-level diffs.
- Set `cell` to the terminal cell size in pixels to get Mermaid diagrams as PNG pictures instead of text.

The contracts behind these functions are in [`specs/01-architecture.md`](../specs/01-architecture.md).
