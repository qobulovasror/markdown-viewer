# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Security
- Control characters in documents are shown as visible symbols (`␛`) instead of reaching the
  terminal, so a document cannot change the title, clipboard or screen via escape sequences.
- Opening a local file or a non-web URL from a link (scripts, apps, custom schemes) asks for
  confirmation first. Web links, images and PDFs open directly.
- Windows: links open through `rundll32` instead of `cmd /C start`, which interpreted `&` in URLs.

### Added
- Open documents from the web: `mdvw https://…/notes.md`, `mdvw gh:owner/repo` (the README),
  `gh:owner/repo/path.md` and `github.com/…` URLs (`blob` and `tree` pages too). Relative links
  in them resolve against the URL, and web links to documents open in the viewer. Built with the
  default `remote` cargo feature.
- Several files on the command line: `}` / `{` step through them (or through the folder), and
  print mode renders each with a `==> name <==` header.
- `mdvw file.md#heading` opens at that heading.
- `O` opens the focused link with the browser or system app.
- Long code lines can scroll sideways instead of wrapping: `W` toggles, `←`/`→` (or `<`/`>`)
  scroll; config `wrap_code = false` makes it the default.
- `/` in the contents panel filters headings.
- `S` searches every file of the list or folder and opens the chosen hit.
- `Y` copies the focused link, or `file#heading` for the current section.
- Recently opened files: `R` in the viewer, `mdvw --recent` on the command line.
- Custom themes from `~/.config/mdvw/themes/<name>.toml` (a built-in base plus color overrides),
  usable with `--theme`, the config file and `T`.
- `remote_images = true` downloads web images, including those of web documents.
- In directory mode, watch (`w`) also keeps the file list up to date.

### Fixed
- `e` opens the editor at the current heading in every format (setext, Org, AsciiDoc, RST), and
  passes the line the way VS Code, Zed, Sublime and Kate expect.
- Key bindings can be changed with a `[keys]` table in the config file.
- Count prefixes: `5j`, `3]`, `42G` (line 42), `50%`, `3y` (copy code block 3).
- Forward navigation (`L`), with `H` as another key for back.
- Search input has a movable cursor, readline keys (`Ctrl-a/e/w/k/u`) and history (`↑`/`↓`).
  The fuzzy finder input uses the same editing keys.
- The help screen scrolls on small terminals and lists the active key bindings.

### Changed
- `←` no longer focuses the side panel (`h` still does); it scrolls code sideways.
- `Esc` no longer quits; it clears the search, link focus or count. Use `q` to quit, or bind
  `quit = ["q", "esc"]` for the old behavior.
- Unknown keys in the config file are reported as warnings instead of errors.

## [0.1.0] - 2026-10-02

First release.

### Rendering
- Markdown (CommonMark + GFM): headings, emphasis, lists, task lists, tables with alignment and
  column fitting, block quotes, admonitions, footnotes, definition lists, front matter.
- Org, AsciiDoc, reStructuredText (including grid and simple tables) and plain text.
- Syntax-highlighted code blocks; wrapped code lines continue with `↪`.
- Mermaid flowcharts and sequence diagrams drawn as box art.
- LaTeX math as Unicode, emoji shortcodes, common inline HTML.
- Inline images on Kitty, iTerm2, WezTerm and Ghostty.
- Five themes with automatic light/dark selection and 256/16-color fallback. `NO_COLOR` is honored.

### Viewer
- Vim/less keys, a table of contents, regex search, link navigation with history.
- A directory mode with a file panel, plus a fuzzy file finder.
- Bookmarks, a remembered reading position, live reload, copying a code block, `$EDITOR`
  integration, focus mode.

### CLI
- `-p` print mode with OSC 8 hyperlinks, stdin input, `--format`, `--theme`, `--width`.
- Shell completions (`--completions`) and a man page (`--man`).

[0.1.0]: https://github.com/qobulovasror/markdown-viewer/releases/tag/v0.1.0
