# mdv

A fast terminal viewer for Markdown, Org, AsciiDoc, reStructuredText and plain text.
It renders documents with colors, tables, code highlighting and clickable links. You can read in a
full-screen viewer with a table of contents, search and live reload, or print the rendered text to
stdout for pipes and scripts.

## Features

- **Rendering**: headings, emphasis, inline code, lists (nested, ordered, task lists), tables
  with alignment and column fitting, block quotes, GitHub admonitions (`> [!NOTE]`), footnotes,
  definition lists, horizontal rules, front matter.
- **Code blocks** are syntax highlighted (bat's syntax set). Languages are detected from the fence
  info or a shebang line.
- **Links** are clickable (OSC 8) in print mode. In the viewer, follow them with `Tab`/`Enter`.
  Local documents open in place, so you can browse a docs folder.
- **Images** appear inline on terminals with the Kitty, iTerm2 or Sixel graphics protocol.
  Other terminals show `[🖼 alt]`.
- **Math**: `$...$` and `$$...$$` are rendered as Unicode (`\sum_{i=1}^n x_i^2` → `∑ᵢ₌₁ⁿ xᵢ²`).
- **Emoji** shortcodes (`:rocket:` → 🚀) and common inline HTML (`<kbd>`, `<sup>`, `<img>`, `<a>`).
- **Formats**: `.md`, `.org`, `.adoc`, `.rst`, `.txt`. Override with `--format`.
- **Viewer**: vim/less keys, table of contents, regex search with smart-case, link navigation,
  history, bookmarks, copy a code block, `$EDITOR` integration, watch mode, a remembered reading
  position, focus (max-width) mode, a directory mode with a file panel and a fuzzy finder.
- **Themes**: `dark`, `light`, `dracula`, `nord`, `gruvbox`. The light/dark default follows the
  terminal background. Colors fall back to 256/16 colors on older terminals. `NO_COLOR` is honored.

## Install

```sh
cargo install --path .
```

Prebuilt binaries are attached to GitHub releases (see `.github/workflows/release.yml`). A Homebrew
formula template is in `packaging/mdv.rb`.

## Usage

```sh
mdv README.md            # open the viewer
mdv docs/                # directory mode: file panel + README/index
mdv -p README.md         # print rendered output
mdv -w notes.md          # live reload while you edit
curl -s https://example.com/x.md | mdv   # read from stdin
mdv -p --width 80 --color always x.md | less -R
```

| Option | Description |
|---|---|
| `-p, --print` | Print to stdout instead of opening the viewer (default when stdout is not a terminal) |
| `-w, --watch` | Reload when the file changes |
| `-t, --theme <name>` | Color theme |
| `-f, --format <fmt>` | Input format: `md`, `org`, `adoc`, `rst`, `txt` |
| `--width <n>` | Print width / maximum text width in the viewer |
| `--toc` | Open the table of contents on start |
| `--color <when>` | `auto`, `always`, `never` |
| `--no-hyperlinks` | Disable OSC 8 links |
| `--no-config` | Ignore the config file |

## Keys

| Key | Action |
|---|---|
| `j` `k` `↓` `↑` | Scroll one line |
| `d` `u`, `Ctrl-d` `Ctrl-u` | Half page down / up |
| `Space` `b`, `PgDn` `PgUp` | Page down / up |
| `g` `G`, `Home` `End` | Top / bottom |
| `]` `[` | Next / previous heading |
| `t` | Table of contents (`h`/`l` move focus between panel and text) |
| `F` | File panel |
| `o`, `Ctrl-p` | Fuzzy find and open a file |
| `/` `n` `N` | Search (regex, smart-case), next / previous match |
| `Tab` `Shift-Tab` | Next / previous link |
| `Enter` | Open the focused link |
| `Backspace`, `Ctrl-o` | Go back |
| `y` + `1-9`, `yy` | Copy code block *n* / the block on screen |
| `m` + letter, `'` + letter | Set / jump to a bookmark |
| `e` | Edit in `$EDITOR` at the current heading |
| `w` | Toggle watch mode |
| `z` | Toggle focus width / full width |
| `T` | Cycle theme |
| `i` | Document info and front matter |
| `M` | Toggle mouse capture (to select text with the mouse) |
| `r` | Reload |
| `?` | Help |
| `q`, `Esc` | Quit (`Esc` first clears the search or link focus) |

The mouse wheel scrolls. Clicking a link opens it, and clicking a panel entry jumps to it.

## Configuration

`~/.config/mdv/config.toml` (or `$XDG_CONFIG_HOME/mdv/config.toml`). Every key is optional:

```toml
theme = "nord"          # unset: auto light/dark
max_width = 100         # viewer text width, 0 = full width
mouse = true
watch = false
hyperlinks = true
toc = false             # open the table of contents on start
remember_position = true
images = true
```

Reading positions and bookmarks are stored in `~/.local/state/mdv/state.json`.

## Development

```sh
cargo test               # unit, snapshot and randomized tests
cargo bench              # parse/layout benchmarks
```

The code is split into stages: `parser` (format → AST), `layout` (AST → width-bounded styled
lines), `render` (ANSI) and `tui` (viewer). To add a format, add a parser that produces the AST.
