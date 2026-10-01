# mrk

Render Markdown beautifully in the terminal: syntax-highlighted code, tables, alerts, footnotes, themes, and Mermaid diagrams drawn as real images in Ghostty, kitty and WezTerm, and with Sixel in foot, xterm, Konsole and others (box-drawing text elsewhere).

## Install

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/vmeyet/mrk-cli/releases/latest/download/mrk-cli-installer.sh | sh
brew install vmeyet/tap/mrk
cargo install --locked --git https://github.com/vmeyet/mrk-cli   # Rust 1.92 or newer
```

Release builds cover macOS and Linux on x86_64 and arm64.
Linux also gets static musl builds, which the shell installer picks on Alpine or when glibc is too old.
`mrk update` installs the latest release tag with cargo when it is newer.
When brew installed mrk, `mrk update` runs `brew upgrade vmeyet/tap/mrk` instead.

## Use

```sh
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
| `--images auto\|always\|never` | | `images` | `auto`: pictures when the terminal speaks the kitty graphics protocol or Sixel, not under tmux or screen; `always` tries inside them too; piped output never gets pictures |
| `--align center\|left` | `MRK_ALIGN` | `align` | `center`: the text column sits in the middle of a wide window; piped output is never centred |
| `--color auto\|always\|never` | `NO_COLOR` | | `auto`: colour on a tty; without colour there are no hyperlinks, so links print their target as ` <url>` |
| `-p`, `--pager` | | `pager` | off; the built-in pager keeps diagrams as images and stays open until `q`; piped output is never paged |
| | `MRK_PAGER` | | the command `-p` pipes into instead of the built-in pager (diagrams as text); setting it does not turn paging on |
| `--completions SHELL` | | | |

Flags win over the environment, the environment over the config file.
A bad flag, environment value or config file exits with 2, any other failure with 1.

The config file is the first of these that exists:

1. `$XDG_CONFIG_HOME/mrk/config.toml` when `XDG_CONFIG_HOME` is set, `~/.config/mrk/config.toml` otherwise
2. `~/Library/Application Support/mrk/config.toml` on macOS


```toml
theme = "tokyo-night"
width = 90
images = "auto"
align = "left"
pager = true
```

## Pager keys

| Keys | Action |
|---|---|
| `j`, `↓`, `Enter` / `k`, `↑` | down / up a row |
| `Space`, `f`, `Page Down`, `Ctrl-f` / `b`, `Page Up`, `Ctrl-b` | down / up a page |
| `d`, `Ctrl-d` / `u`, `Ctrl-u` | down / up half a page |
| `g`, `Home` / `G`, `End` | top / bottom |
| `/` | search; then `Enter` runs it, `Esc` cancels, `Backspace` erases |
| `n` / `N` | next / previous match |
| `q`, `Esc`, `Ctrl-c` | quit |

`mrk --help` prints the same list, from the pager's own key table.

## Completions and man page

```sh
mrk --completions zsh > ~/.zfunc/_mrk                          # with fpath+=~/.zfunc before compinit
mrk --completions bash > ~/.local/share/bash-completion/completions/mrk
mrk --completions fish > ~/.config/fish/completions/mrk.fish
mkdir -p ~/.local/share/man/man1 && mrk --man > ~/.local/share/man/man1/mrk.1   # then: man mrk
```

## Library

Render Markdown into styled lines and pictures from Rust, without the CLI and terminal dependencies; the package is `mrk-cli`, the crate stays `mrk`:

```toml
mrk-cli = { git = "https://github.com/vmeyet/mrk-cli", default-features = false }
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

## License

MIT.
Mermaid labels fall back on an embedded Latin subset of [DejaVu Sans](https://dejavu-fonts.github.io/), renamed `mrk Sans`, under its own license in `assets/fonts/LICENSE-DejaVu`.
