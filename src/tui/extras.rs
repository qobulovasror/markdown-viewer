//! Reading comforts: saved positions, bookmarks, clipboard, watch, editor.

use std::process::Command;

use super::app::App;
use crate::clipboard;
use crate::links::Target;
use crate::parser::Format;
use crate::state::Position;
use crate::watch::FileWatcher;

impl App {
    /// Width-independent description of the current scroll position.
    pub fn current_position(&self) -> Position {
        match self.current_heading() {
            Some(h) => {
                let entry = &self.rendered.headings[h];
                Position {
                    heading: Some(entry.id.clone()),
                    offset: self.scroll.saturating_sub(entry.line),
                }
            }
            None => Position {
                heading: None,
                offset: self.scroll,
            },
        }
    }

    pub fn line_of(&self, pos: &Position) -> usize {
        let base = pos
            .heading
            .as_deref()
            .and_then(|id| self.rendered.anchor_line(id))
            .unwrap_or(0);
        base + pos.offset
    }

    /// Persists the reading position of the current file.
    pub fn save_state(&mut self) {
        if !self.remember || self.rendered.lines.is_empty() {
            return;
        }
        let Some(path) = self.source.path.clone() else {
            return;
        };
        let pos = self.current_position();
        self.state.entry(&path).position = pos;
        let _ = self.state.save();
    }

    pub fn set_mark(&mut self, c: char) {
        let Some(path) = self.source.path.clone() else {
            self.notify("Bookmarks need a file");
            return;
        };
        let pos = self.current_position();
        self.state.entry(&path).bookmarks.insert(c, pos);
        let _ = self.state.save();
        self.notify(format!("Bookmark '{c}' set"));
    }

    pub fn jump_mark(&mut self, c: char) {
        let pos = self
            .source
            .path
            .as_deref()
            .and_then(|p| self.state.get(p))
            .and_then(|f| f.bookmarks.get(&c))
            .cloned();
        match pos {
            Some(pos) => {
                self.record();
                let line = self.line_of(&pos);
                self.scroll_to(line);
                self.notify(format!("Bookmark '{c}'"));
            }
            None => self.notify(format!("No bookmark '{c}'")),
        }
    }

    /// Copies code block `n` (1-based), or the first one visible on screen.
    pub fn copy_code(&mut self, n: Option<usize>) {
        let blocks = &self.rendered.code_blocks;
        let idx = match n {
            Some(n) => n.checked_sub(1).filter(|&i| i < blocks.len()),
            None => {
                let top = self.scroll;
                let bottom = top + self.view_height;
                // A block containing the top line, else the first starting on screen.
                self.rendered
                    .lines
                    .get(top)
                    .and_then(|l| l.code)
                    .or_else(|| blocks.iter().position(|b| b.line >= top && b.line < bottom))
            }
        };
        let Some(i) = idx else {
            self.notify(match n {
                Some(n) => format!("No code block [{n}]"),
                None => "No code block on screen".into(),
            });
            return;
        };
        let code = blocks[i].code.clone();
        match clipboard::copy(&code) {
            Ok(via) => self.notify(format!(
                "Copied code block [{}] ({} lines) to {via}",
                i + 1,
                code.lines().count()
            )),
            Err(e) => self.notify(format!("Copy failed: {e}")),
        }
    }

    /// Copies the focused link's target, or a link to the current section.
    pub fn copy_link(&mut self) {
        let text = match self.focused_link_id() {
            Some(id) => match self.link_target(&self.rendered.links[id]) {
                Target::Anchor(a) => format!("{}#{a}", self.source.location()),
                Target::Document(p, Some(a)) => format!("{}#{a}", p.display()),
                Target::Document(p, None) => p.display().to_string(),
                Target::External(u) => u,
            },
            None => {
                let mut loc = self.source.location();
                if let Some(h) = self.current_heading() {
                    loc = format!("{loc}#{}", self.rendered.headings[h].id);
                }
                loc
            }
        };
        match clipboard::copy(&text) {
            Ok(via) => self.notify(format!("Copied {text} to {via}")),
            Err(e) => self.notify(format!("Copy failed: {e}")),
        }
    }

    pub fn toggle_watch(&mut self) {
        if self.watcher.take().is_some() {
            self.folder_watcher = None;
            self.notify("Watch off");
            return;
        }
        self.start_watch();
        self.rewatch_folder();
    }

    /// Follows the folder in directory mode while watch is on.
    pub(super) fn rewatch_folder(&mut self) {
        self.folder_watcher = None;
        if self.watcher.is_none() || !self.scanned {
            return;
        }
        if let Some(root) = self.root.clone() {
            match FileWatcher::folder(&root) {
                Ok(w) => self.folder_watcher = Some(w),
                Err(e) => self.notify(format!("{e:#}")),
            }
        }
    }

    fn start_watch(&mut self) {
        let Some(path) = self.source.path.clone() else {
            self.notify("Only local files can be watched");
            return;
        };
        match FileWatcher::new(&path) {
            Ok(w) => {
                self.watcher = Some(w);
                self.notify("Watching for changes");
            }
            Err(e) => self.notify(format!("{e:#}")),
        }
    }

    /// Re-targets an active watcher after switching documents.
    pub(super) fn rewatch(&mut self) {
        if self.watcher.take().is_some() {
            self.start_watch();
        }
    }

    pub fn toggle_full_width(&mut self) {
        if self.max_width == 0 {
            self.max_width = self.focus_width;
            self.notify(format!("Focus mode: {} columns", self.max_width));
        } else {
            self.focus_width = self.max_width;
            self.max_width = 0;
            self.notify("Full width");
        }
    }

    /// 1-based source line of the current heading, for `$EDITOR +N`.
    pub fn source_line(&self) -> usize {
        let Some(h) = self.current_heading() else {
            return 1;
        };
        let key = letters(&self.rendered.headings[h].title);
        let nth = self.rendered.headings[..h]
            .iter()
            .filter(|e| letters(&e.title) == key)
            .count();
        heading_candidates(&self.source.text, self.source.format)
            .into_iter()
            .filter(|(_, k)| *k == key)
            .nth(nth)
            .map_or(1, |(i, _)| i + 1)
    }

    /// Runs the editor on the current file; the caller suspends the terminal.
    pub fn run_editor(&mut self) {
        let Some(path) = self.source.path.clone() else {
            self.notify("Only local files can be edited");
            return;
        };
        let editor = std::env::var("VISUAL")
            .or_else(|_| std::env::var("EDITOR"))
            .unwrap_or_else(|_| if cfg!(windows) { "notepad" } else { "vi" }.into());
        let mut parts = editor.split_whitespace();
        let Some(program) = parts.next() else {
            return;
        };
        let mut cmd = Command::new(program);
        cmd.args(parts);
        let name = std::path::Path::new(program)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        cmd.args(editor_args(name, &path, self.source_line()));
        match cmd.status() {
            Ok(_) => match self.reload() {
                Ok(()) => self.notify("Reloaded after editing"),
                Err(e) => self.notify(format!("Reload failed: {e:#}")),
            },
            Err(e) => self.notify(format!("Cannot run {program}: {e}")),
        }
    }
}

/// Arguments that open `path` at `line` in a known editor (else just the path).
fn editor_args(editor: &str, path: &std::path::Path, line: usize) -> Vec<std::ffi::OsString> {
    let file = path.as_os_str().to_owned();
    let mut at = file.clone();
    at.push(format!(":{line}"));
    match editor.to_ascii_lowercase().as_str() {
        "vi" | "vim" | "nvim" | "gvim" | "nano" | "hx" | "helix" | "emacs" | "emacsclient"
        | "micro" | "kak" | "gedit" | "joe" | "mg" | "ne" | "vis" | "jed" => {
            vec![format!("+{line}").into(), file]
        }
        "code" | "code-insiders" | "codium" | "cursor" | "windsurf" => vec!["-g".into(), at],
        "subl" | "sublime_text" | "zed" | "zeditor" => vec![at],
        "kate" => vec!["-l".into(), line.to_string().into(), file],
        _ => vec![file],
    }
}

/// Lowercase letters and digits: heading markup (`#`, `*`, `=`, emphasis) drops out.
fn letters(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// A line of one repeated punctuation character (setext / RST underline).
fn is_rule(line: &str) -> bool {
    let t = line.trim();
    let mut chars = t.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_punctuation() && t.len() >= 2 && chars.all(|d| d == c))
}

/// Source lines (0-based) that look like headings in `format`, with their
/// letters; code blocks are skipped.
fn heading_candidates(text: &str, format: Format) -> Vec<(usize, String)> {
    let (fences, prefixes, underlines): (&[&str], &[&str], bool) = match format {
        Format::Markdown => (&["```", "~~~"], &["#", "<h"], true),
        Format::Org => (&["#+begin_", "#+end_"], &["*"], false),
        Format::AsciiDoc => (&["----", "....", "```"], &["=", "#"], false),
        Format::Rst => (&[], &[], true),
        Format::Text => (&[], &[], false),
    };
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim_start();
        let lower = t.to_ascii_lowercase();
        let marker = fences.iter().copied().find(|m| lower.starts_with(m));
        match (fence, marker) {
            (None, Some(m)) if m != "#+end_" => {
                fence = Some(m);
                continue;
            }
            (Some(f), Some(m)) if f == m || (f.starts_with("#+") && m.starts_with("#+")) => {
                fence = None;
                continue;
            }
            (Some(_), _) => continue,
            _ => {}
        }
        if t.is_empty() || is_rule(t) {
            continue;
        }
        let prefixed = prefixes.iter().any(|p| lower.starts_with(p));
        let underlined = underlines && lines.get(i + 1).is_some_and(|n| is_rule(n));
        if prefixed || underlined {
            out.push((i, letters(t)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_of(text: &str, format: Format, title: &str, nth: usize) -> Option<usize> {
        let key = letters(title);
        heading_candidates(text, format)
            .into_iter()
            .filter(|(_, k)| *k == key)
            .nth(nth)
            .map(|(i, _)| i + 1)
    }

    #[test]
    fn finds_headings_in_every_format() {
        let md = "Intro\n=====\n\n```\n# Not\n```\n\n## Use **it**\n\nSub\n---\n";
        assert_eq!(line_of(md, Format::Markdown, "Intro", 0), Some(1));
        assert_eq!(line_of(md, Format::Markdown, "Not", 0), None);
        assert_eq!(line_of(md, Format::Markdown, "Use it", 0), Some(8));
        assert_eq!(line_of(md, Format::Markdown, "Sub", 0), Some(10));
        // A list item and a thematic break do not count in Markdown.
        let md2 = "* Install\n\n----\n\n## Install\n";
        assert_eq!(line_of(md2, Format::Markdown, "Install", 0), Some(5));
        let org = "#+TITLE: x\n* One\n#+begin_src sh\n* fake\n#+end_src\n** Two\n";
        assert_eq!(line_of(org, Format::Org, "Two", 0), Some(6));
        assert_eq!(line_of(org, Format::Org, "fake", 0), None);
        let adoc = "= Doc\n\n[source]\n----\n== fake\n----\n\n== Part\n";
        assert_eq!(line_of(adoc, Format::AsciiDoc, "Part", 0), Some(8));
        assert_eq!(line_of(adoc, Format::AsciiDoc, "fake", 0), None);
        let rst = "=====\nTitle\n=====\n\nSection\n-------\n\nSection\n-------\n";
        assert_eq!(line_of(rst, Format::Rst, "Title", 0), Some(2));
        assert_eq!(line_of(rst, Format::Rst, "Section", 1), Some(8));
    }

    #[test]
    fn editor_line_arguments() {
        let p = std::path::Path::new("a.md");
        let args = |e| {
            editor_args(e, p, 7)
                .into_iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(args("nvim"), ["+7", "a.md"]);
        assert_eq!(args("code"), ["-g", "a.md:7"]);
        assert_eq!(args("zed"), ["a.md:7"]);
        assert_eq!(args("notepad"), ["a.md"]);
    }
}
