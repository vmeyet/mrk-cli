# Library

mrk's renderer is a Rust library without any terminal dependency.
It turns Markdown into styled lines and PNG pictures; printing them is up to you.

The package is `mrk-cli` and the crate is `mrk`.
The default `cli` feature adds the binary and `mrk::terminal`, which detects the terminal, writes a `Document` with its escape sequences and runs the pager.
Turn it off to leave them out with their dependencies:

```toml
mrk-cli = { version = "0.11", default-features = false }
```

```rust
use mrk::document::{Block, Settings};
use mrk::{markdown, theme};

let source = "# Notes\n\nSee the [guide](docs/guide.md).\n\n- one\n- two\n";
let mut settings = Settings::default();
settings.width = 60;
settings.theme = theme::find("tokyo-night").unwrap_or(theme::MRK_DARK);
for section in markdown::render_sections(source, &settings) {
    println!("{:?}, lines {}-{}", section.kind, section.first_line, section.last_line);
    for block in &section.blocks {
        if let Block::Lines(lines) = block {
            lines.iter().for_each(|line| println!("  {}", line.printable()));
        }
    }
}
```

- `Settings::default()` lays out 80 columns of `mrk-dark` text, without pictures or clickable links; set only the fields you need.
  `Settings` is `#[non_exhaustive]`, as are the other types a later release may extend: start from the default rather than a struct literal.
- `markdown::render` gives the whole `Document`.
- `markdown::render_sections` cuts it into top-level sections, each tagged with its kind and the source lines it came from.
- `document::highlight` restyles character ranges of a section's text, for search or word-level diffs.
- `diff::sections` pairs the sections of two versions of a file as `Same`, `Added`, `Removed` or `Changed`, a changed pair with the char ranges of its changed words on each side, for `highlight`.
- Set `cell` to the terminal cell size in pixels to get Mermaid diagrams as PNG pictures instead of text.
- Mermaid layout runs on the calling thread, bounded by a per-diagram and a per-document budget, so the same input always gives the same pictures and box-drawing text; set `diagram_deadline` to stop laying diagrams out at that moment, each on a thread of its own that keeps running unseen once given up on.
- Set `link_base` to a `link::LinkBase` to make relative targets clickable: `LinkBase::new("https://gitlab.com/group/project/-/merge_requests/7")` accepts only an absolute http or https URL in printable ASCII, and `/uploads/a.pdf`, `docs/x.md`, `../x`, `?q`, `#f` and `//host/x` resolve against it as a browser resolves the links of that page.
  `/a` resolves from the page's host, unless `rooted_at(prefix, root_url)` gives the root-relative targets under `prefix` a root of their own: GitLab writes uploads relative to the project, so `.rooted_at("/uploads/", "https://gitlab.com/group/project")` turns `/uploads/ab/x.pdf` into `https://gitlab.com/group/project/uploads/ab/x.pdf`, and `.rooted_at("/", …)` serves a site under a sub-path.
  Without a base they stay unclickable, their target printed after the text.
- `Settings` is `Clone`, not `Copy`, since a `LinkBase` owns its URLs: clone it to change a field for another render.
- Set `jumbo_title` to draw level-1 headings two rows tall: `Picture` (with `cell` set) gives a picture per line, its text in `concealed_text`; `DoubleHeight` gives `Block::DoubleHeight` lines, for terminals that draw DEC double-height lines.

## Printing safely

Markdown is untrusted input and a terminal interprets escape sequences: a README can hold `\x1b]52;c;…\x07`, which writes the clipboard, or a `javascript:` link.
A `Document` keeps the text as the input wrote it, so print it through `mrk::sanitize`, which every build includes:

- `Line::printable` and `document::printable` give the text with every control, DEL, bidi and invisible mark removed and tabs expanded; `Line::plain` and `document::plain` keep them, for searching and comparing only.
- `sanitize::text` does the same for any other string you print: a span's text, a picture's `alt` or `concealed_text`.
- `sanitize::link` gives the target to put in an OSC 8 hyperlink, or `None` when the link must not be clickable (a relative path when no `link_base` is set, `javascript:`, a `file:` link to another host, over 2,048 bytes); `markdown` prints such a target after the link text, hyperlinks or not.
- A panic inside a diagram renderer is caught and the diagram shows as source, but the panic hook still runs and the default one prints a report that quotes the diagram: install your own hook that stays quiet while `mermaid::is_catching()` and sanitizes the rest.

The contracts behind these functions are in [`specs/01-architecture.md`](../specs/01-architecture.md).
