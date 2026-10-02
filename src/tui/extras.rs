//! Reading comforts: saved positions, bookmarks, clipboard, watch, editor.

use std::process::Command;

use super::app::{App, HistoryEntry};
use crate::clipboard;
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
                self.history.push(HistoryEntry {
                    source: self.source.clone(),
                    scroll: self.scroll,
                });
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

    pub fn toggle_watch(&mut self) {
        if self.watcher.take().is_some() {
            self.notify("Watch off");
            return;
        }
        self.start_watch();
    }

    fn start_watch(&mut self) {
        let Some(path) = self.source.path.clone() else {
            self.notify("Cannot watch stdin");
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
        let title = &self.rendered.headings[h].title;
        let nth = self.rendered.headings[..h]
            .iter()
            .filter(|e| &e.title == title)
            .count();
        let mut in_fence = false;
        self.source
            .text
            .lines()
            .enumerate()
            .filter(|(_, l)| {
                let t = l.trim_start();
                if t.starts_with("```") || t.starts_with("~~~") {
                    in_fence = !in_fence;
                    return false;
                }
                !in_fence
                    && t.starts_with('#')
                    && t.trim_start_matches('#')
                        .trim()
                        .trim_end_matches('#')
                        .trim()
                        == title
            })
            .nth(nth)
            .map_or(1, |(i, _)| i + 1)
    }

    /// Runs the editor on the current file; the caller suspends the terminal.
    pub fn run_editor(&mut self) {
        let Some(path) = self.source.path.clone() else {
            self.notify("stdin cannot be edited");
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
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if matches!(
            name,
            "vi" | "vim" | "nvim" | "nano" | "hx" | "emacs" | "micro" | "kak"
        ) {
            cmd.arg(format!("+{}", self.source_line()));
        }
        match cmd.arg(&path).status() {
            Ok(_) => match self.reload() {
                Ok(()) => self.notify("Reloaded after editing"),
                Err(e) => self.notify(format!("Reload failed: {e:#}")),
            },
            Err(e) => self.notify(format!("Cannot run {program}: {e}")),
        }
    }
}
