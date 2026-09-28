# mrk

Render Markdown beautifully in the terminal: syntax-highlighted code, tables, alerts, footnotes, themes, and Mermaid diagrams drawn as real images in Ghostty, kitty and WezTerm (box-drawing text elsewhere).

```sh
cargo install --git https://github.com/vmeyet/mrk-cli
mrk update        # rebuild from the latest commit; dependencies stay built in the cache
mrk README.md
cat notes.md | mrk
mrk --theme catppuccin-mocha notes.md
mrk --list-themes
```

## Options

| Flag | Env | Config key | Default |
|---|---|---|---|
| `--theme NAME` | `MRK_THEME` | `theme` | `mrk-dark` / `mrk-light` from the terminal background |
| `--width N` | `MRK_WIDTH` | `width` | terminal width, capped at 100 |
| `--images auto\|always\|never` | | `images` | `auto`: pictures when the terminal speaks the kitty graphics protocol, not under tmux |
| `--align center\|left` | `MRK_ALIGN` | `align` | `center`: the text column sits in the middle of a wide window; piped output is never centred |
| `--color auto\|always\|never` | `NO_COLOR` | | `auto`: colour on a tty |
| `--completions SHELL` | | | |

Config lives in `~/.config/mrk/config.toml`:

```toml
theme = "tokyo-night"
width = 90
images = "auto"
align = "left"
```

## Develop

```sh
scripts/check     # fmt, clippy, tests, cargo-deny, machete
cargo run -- tests/fixtures/showcase.md
```

Design and rules: `AGENTS.md` and `specs/`.
