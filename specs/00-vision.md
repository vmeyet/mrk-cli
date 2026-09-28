# 00 · Vision

`mrk README.md` prints the document so it reads like a well-set page: calm colour, clear hierarchy, code that looks like an editor, diagrams that look like diagrams.
It replaces `glow` for everyday reading, and adds Mermaid.

## Goals

1. **Beautiful by default.** No `#` or `*` left on screen; hierarchy comes from colour, weight and space.
2. **Fast.** A 1,000-line README renders in under 50 ms; a file without code or Mermaid never loads syntect or the diagram engine.
3. **Safe.** Rendering a hostile file can do nothing but print text (`02-security.md`).
4. **Themeable.** A handful of built-in themes, light and dark picked from the terminal background, one config file.

## Non-goals (for now)

A pager, a TUI, watching files, fetching remote images or URLs, HTML rendering.

## Usage

```
mrk FILE            render a file
mrk                 render stdin (cat notes.md | mrk)
mrk --theme NAME    pick a theme; mrk --list-themes shows them with a swatch
mrk --width N       wrap width in columns (default: terminal width, capped at 100)
mrk --images auto|always|never   Mermaid as images (auto: when the terminal supports it)
mrk --align center|left          centre the text column in a wide window (default center; piped output is never centred)
mrk --color auto|always|never    auto honours NO_COLOR and a non-tty stdout
mrk --completions zsh
```

Config: `~/.config/mrk/config.toml` with `theme`, `width`, `images`, `align`. Flags win over env (`MRK_THEME`, `MRK_WIDTH`, `MRK_ALIGN`), env over config.

## Visual language

| Element | Rendering |
|---|---|
| H1 | bold, `h1` colour, followed by a `━` rule the width of the content in `subtle` |
| H2 | `▍ ` bar in `h2`, bold title in `h2` |
| H3 | bold `h3` |
| H4–H6 | bold `text`, H6 muted |
| Paragraph | `text`, wrapped to width, one blank line between blocks |
| Emphasis | bold / italic / strikethrough (strike also dimmed) |
| Inline code | `code` colour on `surface`, one space of padding each side |
| Link | underlined `link`, OSC 8 hyperlink; autolinks likewise |
| Image | `▣ alt text` in `muted`, hyperlinked to the source; never fetched |
| Bullets | `•` `◦` `▪` by depth, in `accent`; numbers right-aligned in `accent` |
| Task | `✔` in `success` / `○` in `muted`; done items dimmed |
| Blockquote | `│ ` bar in `subtle`, text italic `muted` |
| Alert (`> [!NOTE]`) | bar and title (`● Note`, `◆ Tip`, `▲ Warning`, …) in the alert colour, body in `text` |
| Code block | panel on `surface` spanning the content width, one column of padding, language label right-aligned in `muted` on the first row, syntax colours from the theme |
| Table | rounded box (`╭┬╮ ├┼┤ ╰┴╯`) in `subtle`, bold header in `accent`, column alignment honoured, cells wrap when the table is too wide |
| Rule | `─` in `subtle` across the width, with a centred `◆` |
| Footnote | `¹`-style marker in `accent`; notes listed at the end under a short rule |
| Front matter | key/value lines in `muted`, then a rule |
| Mermaid | an image sized to the content width and scaled so diagram text matches the terminal font; box-drawing text when images are off |

## Vocabulary

- **Document**: the rendered output, a list of blocks. **Block**: text lines or a picture.
- **Line**: spans that fit the width. **Span**: text with one style and an optional link.
- **Theme**: a palette plus a syntax theme name. **Palette**: the named colours above.
- **Settings**: width, theme and the image cell size, everything a renderer reads.
- **Picture**: a PNG with the cell box it occupies.
- **Capabilities**: what the terminal can do (colour depth, images, cell size, background).
