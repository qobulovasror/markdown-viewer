//! Directory mode file panel and the fuzzy file finder.

use std::path::{Path, PathBuf};

use super::app::{App, Focus, Mode, Panel, Source};
use crate::files;

#[derive(Debug, Default)]
pub struct Finder {
    pub input: String,
    /// Indices into `App::files`, best match first.
    pub results: Vec<usize>,
    pub selected: usize,
}

impl App {
    /// Enables directory mode with the given root and its documents.
    pub fn set_root(&mut self, root: PathBuf, files: Vec<PathBuf>) {
        // Canonical root so relative/absolute document paths compare equal.
        self.root = Some(root.canonicalize().unwrap_or(root));
        self.files = files;
        self.files_selected = self.current_file().unwrap_or(0);
    }

    /// Scans the current file's directory if no root is known yet.
    fn ensure_files(&mut self) -> bool {
        if self.root.is_none() {
            let dir = self
                .source
                .path
                .as_deref()
                .and_then(Path::parent)
                .map(|p| if p.as_os_str().is_empty() { Path::new(".") } else { p })
                .unwrap_or(Path::new("."))
                .to_path_buf();
            let files = files::scan(&dir);
            self.set_root(dir, files);
        }
        if self.files.is_empty() {
            self.notify("No documents found");
            return false;
        }
        true
    }

    pub fn toggle_files(&mut self) {
        if self.panel == Some(Panel::Files) {
            self.panel = None;
            self.focus = Focus::Content;
        } else if self.ensure_files() {
            self.panel = Some(Panel::Files);
            self.focus = Focus::Panel;
        }
    }

    pub fn files_move(&mut self, delta: isize) {
        if !self.files.is_empty() {
            self.files_selected = self
                .files_selected
                .saturating_add_signed(delta)
                .min(self.files.len() - 1);
        }
    }

    pub fn files_offset(&self, height: usize) -> usize {
        self.files_selected
            .saturating_sub(height / 2)
            .min(self.files.len().saturating_sub(height))
    }

    /// Index of the file currently shown, if it is in the list.
    pub fn current_file(&self) -> Option<usize> {
        let root = self.root.as_deref()?;
        let path = self.source.path.as_deref()?;
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let rel = path.strip_prefix(root).ok()?;
        self.files.iter().position(|f| f == rel)
    }

    pub fn open_file(&mut self, idx: usize) {
        let (Some(root), Some(rel)) = (self.root.clone(), self.files.get(idx).cloned()) else {
            return;
        };
        self.files_selected = idx;
        if self.current_file() == Some(idx) {
            return;
        }
        let path = root.join(&rel);
        match Source::from_file(&path) {
            Ok(src) => {
                self.open_source(src);
                self.notify(format!("Opened {}", rel.display()));
            }
            Err(e) => self.notify(format!("{e:#}")),
        }
    }

    pub fn open_finder(&mut self) {
        if !self.ensure_files() {
            return;
        }
        self.mode = Mode::Finder;
        self.finder.input.clear();
        self.finder_update();
    }

    pub fn finder_update(&mut self) {
        self.finder.results = files::fuzzy(&self.files, &self.finder.input);
        self.finder.selected = 0;
    }

    pub fn finder_move(&mut self, delta: isize) {
        let n = self.finder.results.len();
        if n > 0 {
            self.finder.selected = self.finder.selected.saturating_add_signed(delta).min(n - 1);
        }
    }

    pub fn finder_open(&mut self) {
        self.mode = Mode::Normal;
        if let Some(&idx) = self.finder.results.get(self.finder.selected) {
            self.open_file(idx);
        }
    }
}
