# 02 · Security

The input is untrusted: a README from a cloned repo, a paste, piped output.
The terminal is an interpreter: escape sequences can retitle windows, write the clipboard (OSC 52), change colours, or on some terminals worse.
mrk's job is to make rendering a hostile file equivalent to rendering a boring one.

## Rules

1. **One gate to the terminal.** Every span text and every link passes `sanitize` before being written: C0 controls (except tab, expanded to spaces, and newline, which never reaches a span), DEL, C1 controls (U+0080–U+009F), bidi overrides/isolates (U+202A–U+202E, U+2066–U+2069), bidi marks (U+200E, U+200F, U+061C) and the zero-width space (U+200B) are removed: they reorder or disguise text, as in a name that reads as another. The joiners U+200C and U+200D stay, since emoji sequences and Persian or Indic text need them. Tests feed ESC, BEL, OSC 52 and CSI sequences through every block kind.
   Picture sequences (kitty, Sixel) carry only bytes mrk derives from its own raster, never input text; the PNG decoder only reads the PNGs mrk drew.
   A jumbo title's concealed text is span text and passes `sanitize` like any; the double-height and conceal sequences around it are mrk's own.
   Inside tmux, only those picture sequences and the kitty query are wrapped for passthrough; span text never is, and `sanitize` strips the ESC that could open a passthrough of its own.
   Stderr is a terminal too: error reports, warnings and clap's usage errors pass `sanitize` before being written, since they quote file names, arguments, environment values and git's output.
   `sanitize` is part of the library core, not the `cli` feature: a Document holds the input's text as written, so `Line::printable`, `document::printable` and `sanitize::link` are what a library user prints, and `docs/library.md` shows only them.
2. **Links are data.** An OSC 8 target is sanitized, capped at 2,048 bytes, and dropped (text kept) when it contains a character outside printable ASCII after percent-encoding or when its scheme is not `http`, `https`, `mailto` or `file`.
   A `file` link must name no host or `localhost` (`file:///…`, `file://localhost/…`), its path, percent-decoded once, holding no `\` and no empty, `.` or `..` segment (a trailing slash aside), which openers resolve into `//host/…`. A link to another host opens an SMB share on some systems, which sends it the user's credentials.
3. **No network, no surprise reads.** mrk reads the file named on the command line, stdin, its config file, and font files. Markdown images are never fetched or opened; they render as their alt text.
   Fonts are only read, never written: the first diagram or title picture lists `/System/Library/Fonts`, `/Library/Fonts`, `/usr/share/fonts`, `/usr/local/share/fonts`, `~/Library/Fonts`, `~/.local/share/fonts` and `~/.fonts`, four levels deep, and maps the `.ttf`, `.otf`, `.ttc` and `.otc` files of the label families and script fallbacks, or every one of them when a label needs a glyph those miss. No fontconfig file is read.
   Network exception: `mrk update`, and only when the user runs it, asks `git ls-remote --tags` for the latest release tag of the repo and its commit, then runs `brew upgrade` (`brew reinstall` with `--force`) when brew installed the running binary, else `cargo install --git … --rev <commit>` for that commit; rendering never reaches the network.
   Known exception, which its API gives no way to turn off: to measure the labels of pie, gantt and ER diagrams, and any label beyond ASCII, `mermaid-rs-renderer` loads the system fonts through fontdb (on Linux also the fontconfig files `/etc/fonts/fonts.conf`, `/etc/fonts/local.conf`, `$XDG_CONFIG_HOME/fontconfig/fonts.conf` or `$FONTCONFIG_FILE`, and the directories they list), then reads, or writes once, a copy of the face it picked under `$XDG_CACHE_HOME/mmdr/font-cache/` (`~/.cache` without it).
   The bytes it writes come from an installed font, never from the input; the bytes it reads back are whatever that file holds, so another program of the user can make ttf-parser parse a file of its choosing (`deny.toml`).
4. **Bounded work.** Input capped at 8 MiB; a diagram or a title picture is shrunk so its PNG stays within 4096×4096 pixels; SVG rasterisation refuses external `href`s (images, fonts) so a diagram label cannot pull a file off disk; a title enters its SVG as escaped character data, control characters dropped, so it can add no element.
   A Mermaid block reaches neither engine, and shows as source with its note, when it is over 64 KiB, when it reuses a subgraph id or nests subgraphs over 16 deep (mermaid-text's recursion would overflow the stack, which no `catch_unwind` survives), when mermaid-rs-renderer cannot parse it, or when it is over the layout budget below.
   Layout time grows far faster than the source: 500 edges between random nodes, 7.8 KB, took 10.8 s as a picture and 41.5 s as text, and 63 KB of them over 120 s; refused, they take 1 ms and 10 ms.
   So `mermaid/budget.rs` counts, on the parsed diagram and before any layout, what each layout grows with:

   | Diagrams | Counted | Limit | Slowest measured within it |
   |---|---|---|---|
   | flowchart, class, state, ER, requirement, packet, architecture | links: edges, edge labels, subgraphs, and each edge between two nodes already joined once per earlier one | 30 | 0.5 s (28 edges between random nodes) |
   | sequence | messages and notes | 200 | 0.2 s |
   | every other kind | nodes, edges, subgraphs, git commits, C4 shapes and relations | 300 | 0.7 s (sankey; the others under 0.5 s) |

   A time is one diagram through `mermaid::render`, picture then text, the median of three runs of the slowest shape tried at that limit (tangled, labelled and parallel edges, nested subgraphs, class, state, ER, requirement, architecture, sankey, C4, block, mind map, gantt and git diagrams), in a release build on an Apple M5 kept busy by other builds (load 5 to 18), so single runs reached 0.8 s.
   The showcase flowchart takes 40 ms.
   The limits are structural, not a deadline: a renderer cannot be stopped, so a timed-out thread would keep burning CPU.
   A panic inside a renderer is caught, and its report, which quotes the diagram, is dropped instead of reaching stderr (rule 1).

5. **Terminal queries are parsed strictly.** Responses are read from `/dev/tty` with a timeout, bounded in length; bytes between replies (keys typed meanwhile) are skipped, and a malformed reply means "unsupported".
6. **Config is data.** Strict TOML, unknown keys rejected, no paths or commands in it.
   `MRK_PAGER`, set by the user, is split into words (`shell-words`) and run without a shell, so nothing in it expands; the pager's search query goes through `sanitize` like span text.
7. **No `unsafe`**, enforced by `unsafe_code = "forbid"`; `cargo deny check` clean.
