# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/).

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
