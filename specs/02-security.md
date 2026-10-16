# 02 · Security

The input is untrusted: a README from a cloned repo, a paste, piped output.
The terminal is an interpreter: escape sequences can retitle windows, write the clipboard (OSC 52), change colours, or on some terminals worse.
mrk's job is to make rendering a hostile file equivalent to rendering a boring one.

## Rules

1. **One gate to the terminal.** Every span text and every link passes `sanitize` before being written: C0 controls (except tab, expanded to spaces, and newline, which never reaches a span), DEL, C1 controls (U+0080–U+009F), bidi overrides/isolates (U+202A–U+202E, U+2066–U+2069), bidi marks (U+200E, U+200F, U+061C), the zero-width space (U+200B), the word joiner (U+2060), the byte order mark (U+FEFF), the line and paragraph separators (U+2028, U+2029) and tag characters (U+E0000–U+E007F) are removed: they reorder or disguise text, as in a name that reads as another, or break a line the layout never counted; an emoji subdivision flag, built from tag characters, shows as its black flag. The joiners U+200C and U+200D stay, since emoji sequences and Persian or Indic text need them. Tests feed ESC, BEL, OSC 52 and CSI sequences through every block kind.
   Picture sequences (kitty, Sixel) carry only bytes mrk derives from its own raster, never input text; the PNG decoder only reads the PNGs mrk drew.
   A jumbo title's concealed text is span text and passes `sanitize` like any; the double-height and conceal sequences around it are mrk's own.
   Inside tmux, only those picture sequences and the kitty query are wrapped for passthrough; span text never is, and `sanitize` strips the ESC that could open a passthrough of its own.
   Stderr is a terminal too: error reports, warnings and clap's usage errors pass `sanitize` before being written, since they quote file names, arguments, environment values and git's output.
   `sanitize` is part of the library core, not the `cli` feature: a Document holds the input's text as written, so `Line::printable`, `document::printable` and `sanitize::link` are what a library user prints, and `docs/library.md` shows only them.
2. **Links are data.** An OSC 8 target is sanitized, capped at 2,048 bytes, and dropped (text kept) when it contains a character outside printable ASCII after percent-encoding or when its scheme is not `http`, `https`, `mailto` or `file`.
   A `file` link must name no host or `localhost` (`file:///…`, `file://localhost/…`), its path, percent-decoded once, holding no `\` and no empty, `.` or `..` segment (a trailing slash aside), which openers resolve into `//host/…`. A link to another host opens an SMB share on some systems, which sends it the user's credentials.
3. **No network, no surprise reads.** mrk reads the file named on the command line, stdin, its config file, and font files. Markdown images are never fetched or opened; they render as their alt text.
   Fonts are only read, never written: the first diagram or title picture lists `/System/Library/Fonts`, `/Library/Fonts`, `/usr/share/fonts`, `/usr/local/share/fonts`, `~/Library/Fonts`, `~/.local/share/fonts` and `~/.fonts`, four levels deep, following links to files but never to directories, and maps the `.ttf`, `.otf`, `.ttc` and `.otc` files of the label families and script fallbacks, or every one of them when a label needs a glyph those miss. No fontconfig file is read.
   Network exception: `mrk update`, and only when the user runs it, asks `git ls-remote --tags` for the latest release tag of the repo and its commit, then runs `brew upgrade` (`brew reinstall` with `--force`) when brew installed the running binary, else `cargo install --git … --rev <commit>` for that commit; rendering never reaches the network.
   Known exception, which its API gives no way to turn off: to measure the labels of pie, gantt and ER diagrams, and any label beyond ASCII, `mermaid-rs-renderer` loads the system fonts through fontdb (on Linux also the fontconfig files `/etc/fonts/fonts.conf`, `/etc/fonts/local.conf`, `$XDG_CONFIG_HOME/fontconfig/fonts.conf` or `$FONTCONFIG_FILE`, and the directories they list), then reads, or writes once, a copy of the face it picked under `$XDG_CACHE_HOME/mmdr/font-cache/` (`~/.cache` without it).
   The bytes it writes come from an installed font, never from the input; the bytes it reads back are whatever that file holds, so another program of the user can make ttf-parser parse a file of its choosing (`deny.toml`).
4. **Bounded work.** Input capped at 8 MiB; a diagram or a title picture is shrunk so its PNG stays within 4096×4096 pixels; SVG rasterisation refuses external `href`s (images, fonts) so a diagram label cannot pull a file off disk; a title enters its SVG as escaped character data, control characters dropped, so it can add no element.
   A Mermaid block over 64 KiB, or that reuses a subgraph id or nests subgraphs over 16 deep (mermaid-text's recursion would overflow the stack, which no `catch_unwind` survives), reaches neither engine and shows as source with its note.
   Layout time grows far faster than the source: 500 edges between random nodes, 7.8 KB, took 10.8 s as a picture and 41.5 s as text, and 63 KB of them over 120 s.
   So `mermaid/budget.rs` counts what each layout grows with before that engine runs, and a diagram over its count goes to the other engine, then to source with a muted note naming the limit:

   | Engine | Diagrams | Counted | Limit |
   |---|---|---|---|
   | picture, on mermaid-rs-renderer's parse | flowchart, class, state, ER, requirement, architecture | links: nodes, edges, edge labels, subgraphs, and each edge between two nodes already joined once per earlier one | 300 |
   | picture | sequence | participants, messages and notes | 400 |
   | picture | every other kind | nodes, edges, subgraphs, git commits, C4 shapes and relations | 600 |
   | text, on a scan of the source (mermaid-text reads syntax mermaid-rs-renderer refuses) | class | relations (statements with `--` or `..`) | 32 |
   | text | sequence, pie, gantt, timeline, journey, git, mind map, kanban | nothing: drawn row after row, 64 KiB of any took under 30 ms | none |
   | text | every other kind | statements, one more per arrow chained after the first | 160 |

   On top of the counts, both engines together get 2 s per diagram (20 s in a debug build): each runs on a thread of its own, with a 64 MiB stack, and a diagram not drawn in time shows as source (`too slow to lay out`).
   That result is cached like a drawing (`mermaid/cache.rs`), so a pager resize does not start the same work again.
   A thread given up on cannot be stopped: it runs to its end and its result is dropped, which costs CPU only in the pager, since a one-shot `mrk` exits without waiting for it.
   This keeps the middle pure: the thread writes nothing, reads nothing and shares nothing but its result, and the only thing the clock can change is a picture becoming the source.
   Measured on an Apple M5 in a release build, one run each through `mermaid::render`: twelve real diagrams (CI pipeline, auth flow, two state machines, ER, class, sequence, microservices, data pipeline, Kubernetes, module dependencies, incident flow) all draw, as pictures in 0.03 to 1.3 s (the module graph, 130 links), the class diagram as text since mermaid-rs-renderer cannot parse it; the slowest picture within the budget took 1.9 s (100 edges between 50 random nodes), the slowest text 0.7 s.
   Over 120 shapes made to be slow (tangled and labelled edges, nested and flat subgraphs, thousands of nodes or classes, every kind at 64 KiB) all finish within the deadline, refused in at most 1.2 s, the time mermaid-rs-renderer takes to parse 63 KB.
   A panic inside a renderer is caught, and its report, which quotes the diagram, is dropped instead of reaching stderr (rule 1); the pager leaves it out of the report it prints on exit.
   `diff::sections` compares words with a Myers diff, whose time grows with the product of the two sides when they differ throughout: 40,000 words all changed took 1.4 s and 100,000 took 9.6 s, while 100,000 words with one in a hundred changed took 7 ms (release build, Apple M5).
   So a pair of sections over 20,000 words, both sides counted (0.4 s at worst), is marked whole without a diff, and the section and word diffs of one call share a 1 s deadline, past which what is left to compare is marked whole; the clock changes only which words are marked.

5. **Terminal queries are parsed strictly.** Responses are read from `/dev/tty` with a timeout, bounded in length; bytes between replies (keys typed meanwhile) are skipped, and a malformed reply means "unsupported".
6. **Config is data.** Strict TOML, unknown keys rejected, no paths or commands in it.
   `MRK_PAGER`, set by the user, is split into words (`shell-words`) and run without a shell, so nothing in it expands; the pager's search query goes through `sanitize` like span text.
7. **No `unsafe`**, enforced by `unsafe_code = "forbid"`; `cargo deny check` clean.
