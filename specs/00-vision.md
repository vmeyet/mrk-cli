# 00 · Vision

`mrk README.md` prints the document so it reads like a well-set page: calm colour, clear hierarchy, code that looks like an editor, diagrams that look like diagrams.
It replaces `glow` for everyday reading, and adds Mermaid.

## Goals

1. **Beautiful by default.** No `#` or `*` left on screen; hierarchy comes from colour, weight and space.
2. **Fast.** A 1,000-line README renders in under 50 ms; a file without code or Mermaid never loads syntect or the diagram engine.
3. **Safe.** Rendering a hostile file can do nothing but print text (`02-security.md`).
4. **Themeable.** A handful of built-in themes, light and dark picked from the terminal background, one config file.

## Non-goals (for now)

A full TUI, watching files, fetching remote images or URLs, HTML rendering.

## Usage

```
mrk FILE            render a file
mrk                 render stdin (cat notes.md | mrk)
mrk --theme NAME    pick a theme; mrk --list-themes shows them with a swatch
mrk --width N       wrap width in columns (default: terminal width, capped at 100)
mrk --images auto|always|never   Mermaid as images (auto: when the terminal speaks kitty graphics or Sixel, inside tmux with passthrough allowed, not under screen/zellij; always: inside them too)
mrk --jumbo-title                level-1 headings two rows tall (off by default; see Visual language)
mrk --align center|left          centre the text column in a wide window (default center; piped output is never centred)
mrk --color auto|always|never    auto honours NO_COLOR and a non-tty stdout
mrk -p FILE         read in the pager: diagrams stay images, j/k/space/b/g/G scroll, / n N search, q quits
MRK_PAGER="less -R" mrk -p FILE  pipe into that command instead (split into words, no shell), diagrams as text; MRK_PAGER alone never pages
mrk --completions zsh
mrk update          install the latest release tag with cargo when it is newer (--force reinstalls); the only command that uses the network
mrk --man           the man page as roff (hidden from --help)
mrk --list-themes   names with a swatch in a Dark and a Light group, the group of the terminal background marked; bare names one per line when piped or without colour
```

Config: the first of `$XDG_CONFIG_HOME/mrk/config.toml` (or `~/.config/mrk/config.toml`) and, on macOS, `~/Library/Application Support/mrk/config.toml`, with `theme`, `theme_dark`, `theme_light`, `width`, `images`, `align`, `pager`, `jumbo_title`. Flags win over env (`MRK_THEME`, `MRK_WIDTH`, `MRK_ALIGN`), env over config.
Theme: `--theme`, `MRK_THEME` or `theme` wins; else `theme_dark` or `theme_light` after the terminal background (dark when unknown), a side left out being `mrk-dark` or `mrk-light`.
Every theme name given is checked, the pair's too, whatever the background.
`--help` ends with that lookup, the environment, the pager keys and the exit status; the man page carries the same sections.

Exit status: 0 on success, 2 for a usage mistake (a bad flag, environment value or config file, an unknown theme from any of them), 1 for any other failure.
An unknown theme gets a "did you mean" only when the closest name is at most one edit per three typed characters away.

## Visual language

| Element | Rendering |
|---|---|
| H1 | bold, `h1` colour, followed by a `━` rule the width of the content in `subtle` |
| Jumbo H1 (`--jumbo-title`) | the H1 two rows tall, wrapped at half the width, then its rule. With kitty graphics outside tmux: a picture of each line in bold `h1` (regular when only the embedded font is found), drawn under the line's text written concealed, so selecting it copies the words. Else on xterm, Konsole, Windows Terminal, mlterm and iTerm2 outside a multiplexer: double-height text (DECDHL) in the H1 style. Elsewhere, inside lists and quotes, piped or under `MRK_PAGER`: the plain H1 |
| H2 | `▍ ` bar in `h2`, bold title in `h2` |
| H3 | bold `h3` |
| H4–H6 | bold `text`, H6 muted |
| Paragraph | `text`, wrapped to width, one blank line between blocks |
| Emphasis | bold / italic / strikethrough (strike also dimmed) |
| Inline code | `text` mixed 20% toward `code`, on `surface` mixed 5% toward `code`, one space of padding each side |
| Link | underlined `link`, OSC 8 hyperlink; autolinks likewise. Without hyperlinks (piped, no colour, dumb terminal) the text is followed by ` <url>` in `muted`, except when the text already is the URL |
| Image | `▣ alt text` in `muted`, hyperlinked to the source (` <source>` after it without hyperlinks); never fetched |
| Bullets | `•` `◦` `▪` by depth, in `accent`; numbers right-aligned in `accent` |
| Task | `✔` in `success` / `○` in `muted`; done items dimmed |
| Blockquote | `│ ` bar in `subtle`, text italic `muted` |
| Alert (`> [!NOTE]`) | bar and title (`● Note`, `◆ Tip`, `▲ Warning`, …) in the alert colour, body in `text` |
| Code block | panel on `surface` spanning the content width, one column of padding, language label right-aligned in `muted` on the first row, syntax colours from the theme |
| Table | rounded box (`╭┬╮ ├┼┤ ╰┴╯`) in `subtle`, bold header in `accent`, column alignment honoured, cells wrap when the table is too wide |
| Rule | `─` in `subtle` across the width, with a centred `◆` |
| Footnote | `¹`-style marker in `accent`; notes listed at the end under a short rule |
| Front matter | key/value lines in `muted`, then a rule |
| Pager status bar | last row on `surface`: file name in bold `text`, the search prompt or `3/12` in `accent`, `Top`/`Bot`/`42%` in `accent` and a `muted` key hint |
| Search match | current match `surface` on `accent`, bold; other matches `text` on `subtle` |
| Mermaid | an image sized to the content width and scaled so diagram text matches the terminal font; box-drawing text when images are off. In a list or quote the image keeps the indent and bar beside every row, and an item that opens on a diagram has its marker on a line of its own |

## Vocabulary

- **Document**: the rendered output, a list of blocks. **Block**: text lines or a picture.
- **Line**: spans that fit the width. **Span**: text with one style and an optional link.
- **Theme**: a palette plus a syntax theme, one of the `SyntaxTheme` variants that `code` maps onto a two-face theme. **Palette**: the named colours above.
- **Settings**: width, theme, the image cell size, whether links are clickable and how a jumbo title is drawn, everything a renderer reads.
- **Picture**: a PNG with the cell box it occupies and the indent drawn left of each of its rows; a title's picture also carries the text concealed under it.
- **Jumbo title**: a level-1 heading drawn two rows tall, as a picture or as double-height lines.
- **Capabilities**: what the terminal can do (colour depth, graphics, double-height lines, cell size, background).

- **Graphics**: the protocol pictures are drawn with, kitty or Sixel, and the cell size in pixels.
