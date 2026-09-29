# 02 · Security

The input is untrusted: a README from a cloned repo, a paste, piped output.
The terminal is an interpreter: escape sequences can retitle windows, write the clipboard (OSC 52), change colours, or on some terminals worse.
mrk's job is to make rendering a hostile file equivalent to rendering a boring one.

## Rules

1. **One gate to the terminal.** Every span text and every link passes `terminal::sanitize` before being written: C0 controls (except tab, expanded to spaces, and newline, which never reaches a span), DEL, C1 controls (U+0080–U+009F) and bidi overrides/isolates (U+202A–U+202E, U+2066–U+2069) are removed. Tests feed ESC, BEL, OSC 52 and CSI sequences through every block kind.
2. **Links are data.** An OSC 8 target is sanitized, capped at 2,048 bytes, and dropped (text kept) when it contains a character outside printable ASCII after percent-encoding or when its scheme is not `http`, `https`, `mailto` or `file`.
3. **No network, no surprise reads.** mrk reads the file named on the command line, stdin, and its config file. Markdown images are never fetched or opened; they render as their alt text.
   Network exception: `mrk update`, and only when the user runs it, asks `git ls-remote --tags` for the latest release tag of the repo and runs `cargo install --git … --tag` for it; rendering never reaches the network.
   Known exception: `mermaid-rs-renderer` caches a copy of the diagram's system font under `$XDG_CACHE_HOME/mmdr/font-cache/` for pie, gantt, ER and non-ASCII labels. The cached bytes come from an installed font picked by mrk's font list, never from the input.
4. **Bounded work.** Input capped at 8 MiB; a Mermaid block over 64 KiB falls back to text; a diagram is shrunk so its PNG stays within 4096×4096 pixels; a flowchart that reuses a subgraph id or nests subgraphs over 16 deep is never handed to the text renderer (its recursion would overflow the stack, which no `catch_unwind` survives) and shows as source; SVG rasterisation refuses external `href`s (images, fonts) so a diagram label cannot pull a file off disk.
5. **Terminal queries are parsed strictly.** Responses are read from `/dev/tty` with a timeout, bounded in length, and anything unexpected means "unsupported".
6. **Config is data.** Strict TOML, unknown keys rejected, no paths or commands in it.
   `MRK_PAGER`, set by the user, is split into words (`shell-words`) and run without a shell, so nothing in it expands; the pager's search query goes through `sanitize` like span text.
7. **No `unsafe`**, enforced by `unsafe_code = "forbid"`; `cargo deny check` clean.
