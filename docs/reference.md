# Reference

Everything `mrk --help` and `man mrk` say, in one page.

## Options

| Flag                         | Environment | Config key | Default and notes                                                                                   |
| :--------------------------- | :---------- | :--------- | :-------------------------------------------------------------------------------------------------- |
| `--theme NAME`               | `MRK_THEME` | `theme`    | `mrk-dark` or `mrk-light`, from the terminal background; `--list-themes` shows the names            |
| `--width N`                  | `MRK_WIDTH` | `width`    | the terminal width, capped at 100                                                                    |
| `--images auto\|always\|never` |             | `images`   | `auto`: pictures on terminals that can draw them, inside tmux with passthrough, never under screen or zellij; `always` tries under them too |
| `--align center\|left`        | `MRK_ALIGN` | `align`    | `center`: the text column sits in the middle of a wide window                                       |
| `--color auto\|always\|never`  | `NO_COLOR`  |            | `auto`: colour on a terminal, unless `NO_COLOR` is set                                               |
| `-p`, `--pager`              |             | `pager`    | off; the built-in pager keeps diagrams as images                                                     |
|                              | `MRK_PAGER` |            | a command `-p` pipes into instead of the built-in pager, diagrams as text; it never turns paging on |
| `--completions SHELL`        |             |            | prints the completion script for `bash`, `elvish`, `fish`, `powershell` or `zsh`                    |
| `--list-themes`              |             |            | names with a swatch on a terminal, one name per line when piped                                      |

Piped output is never centred, paged or given pictures.
Without colour there are no hyperlinks, so links print their target as ` <url>` after the text.

## Config file

Flags win over the environment, the environment over the config file.
mrk reads the first of these files that exists:

1. `$XDG_CONFIG_HOME/mrk/config.toml` when `XDG_CONFIG_HOME` is set, `~/.config/mrk/config.toml` otherwise
2. `~/Library/Application Support/mrk/config.toml`, on macOS only

It is strict TOML with five optional keys; an unknown key is an error.

```toml
theme = "tokyo-night"
width = 90
images = "auto"
align = "left"
pager = true
```

## Pager keys

| Keys                                        | Action                                         |
| :------------------------------------------ | :--------------------------------------------- |
| `j`, `↓`, `Enter` / `k`, `↑`                | down / up a row                                |
| `Space`, `f`, `Page Down`, `Ctrl-f` / `b`, `Page Up`, `Ctrl-b` | down / up a page            |
| `d`, `Ctrl-d` / `u`, `Ctrl-u`               | down / up half a page                          |
| `g`, `Home` / `G`, `End`                    | top / bottom                                   |
| `/`                                         | search: `Enter` runs it, `Esc` cancels, `Backspace` erases |
| `n` / `N`                                   | next / previous match                          |
| `q`, `Esc`, `Ctrl-c`                        | quit                                           |

The mouse wheel scrolls on terminals that send it as arrow keys in the alternate screen, and text selection keeps working.

## Exit status

| Code | Meaning                                                              |
| ---: | :------------------------------------------------------------------- |
|    0 | success                                                              |
|    1 | a failure while running, such as a file that cannot be read          |
|    2 | a usage mistake: a bad flag, environment value, config file or theme |

## Shell completions and man page

```sh
mrk --completions zsh > ~/.zfunc/_mrk        # with fpath+=~/.zfunc before compinit
mrk --completions bash > ~/.local/share/bash-completion/completions/mrk
mrk --completions fish > ~/.config/fish/completions/mrk.fish
mkdir -p ~/.local/share/man/man1 && mrk --man > ~/.local/share/man/man1/mrk.1
```
