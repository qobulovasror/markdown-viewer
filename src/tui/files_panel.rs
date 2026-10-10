//! Directory mode file panel and the fuzzy file finder.

use std::path::{Path, PathBuf};

use super::app::{App, Focus, Mode, Panel, Source};
use crate::files;

#[derive(Debug, Default)]
pub struct Finder {
    pub input: super::input::LineInput,
    /// Recently opened files, when the finder lists those instead of `App::files`.
    pub recent: Option<Vec<PathBuf>>,
    /// Indices into the listed paths, best match first.
    pub results: Vec<usize>,
    pub selected: usize,
}

impl App {
    /// Enables directory mode with the given root and its documents.
    pub fn set_root(&mut self, root: PathBuf, files: Vec<PathBuf>) {
        self.set_root_inner(root, files, true);
    }

    fn set_root_inner(&mut self, root: PathBuf, files: Vec<PathBuf>, scanned: bool) {
        // Canonical root so relative/absolute document paths compare equal.
        self.root = Some(root.canonicalize().unwrap_or(root));
        self.files = files;
        self.files_selected = self.current_file().unwrap_or(0);
        self.scanned = scanned;
        self.rewatch_folder();
    }

    /// Re-reads the folder after documents were added or removed.
    pub(super) fn rescan_files(&mut self) {
        let Some(root) = self.root.clone().filter(|_| self.scanned) else {
            return;
        };
        let selected = self.files.get(self.files_selected).cloned();
        self.files = files::scan(&root);
        // Keep the panel cursor where the user left it.
        self.files_selected = selected
            .and_then(|s| self.files.iter().position(|f| *f == s))
            .or_else(|| self.current_file())
            .unwrap_or(0)
            .min(self.files.len().saturating_sub(1));
        if self.mode == Mode::Finder && self.finder.recent.is_none() {
            self.finder_update();
        }
        self.notify(format!(
            "File list updated ({} documents)",
            self.files.len()
        ));
    }

    /// Uses the files named on the command line as the file list.
    pub fn set_file_list(&mut self, files: Vec<PathBuf>) {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let root = root.canonicalize().unwrap_or(root);
        let mut list: Vec<PathBuf> = Vec::new();
        for f in files {
            let abs = f.canonicalize().unwrap_or(f);
            let rel = abs
                .strip_prefix(&root)
                .map(Path::to_path_buf)
                .unwrap_or(abs);
            if !list.contains(&rel) {
                list.push(rel);
            }
        }
        self.set_root_inner(root, list, false);
    }

    /// Opens the file `delta` places away in the list (wrapping around).
    pub fn step_file(&mut self, delta: isize) {
        if !self.ensure_files() {
            return;
        }
        let n = self.files.len() as isize;
        // Off the list (e.g. after following a link): `}` starts at the selection.
        let next = match self.current_file() {
            Some(cur) => (cur as isize + delta).rem_euclid(n) as usize,
            None if delta > 0 => (self.files_selected as isize + delta - 1).rem_euclid(n) as usize,
            None => (self.files_selected as isize + delta).rem_euclid(n) as usize,
        };
        self.open_file(next);
        let label = self.files.get(next).map(|f| f.display().to_string());
        if let Some(f) = label {
            self.notify(format!("{} of {n}: {f}", next + 1));
        }
    }

    /// Scans the current file's directory if no root is known yet.
    pub(super) fn ensure_files(&mut self) -> bool {
        if self.root.is_none() && self.source.url.is_some() {
            self.notify("No file list for a web document");
            return false;
        }
        if self.root.is_none() {
            let dir = self
                .source
                .path
                .as_deref()
                .and_then(Path::parent)
                .map(|p| {
                    if p.as_os_str().is_empty() {
                        Path::new(".")
                    } else {
                        p
                    }
                })
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
        let rel = path.strip_prefix(root).unwrap_or(&path);
        self.files.iter().position(|f| f == rel)
    }

    /// Shows file `idx` of the list; false if it could not be read.
    pub fn open_file(&mut self, idx: usize) -> bool {
        let (Some(root), Some(rel)) = (self.root.clone(), self.files.get(idx).cloned()) else {
            return false;
        };
        self.files_selected = idx;
        if self.current_file() == Some(idx) {
            return true;
        }
        let path = root.join(&rel);
        match Source::from_file(&path) {
            Ok(src) => {
                self.open_source(src);
                self.notify(format!("Opened {}", rel.display()));
                true
            }
            Err(e) => {
                self.notify(format!("{e:#}"));
                false
            }
        }
    }

    pub fn open_finder(&mut self) {
        if !self.ensure_files() {
            return;
        }
        self.mode = Mode::Finder;
        self.finder.recent = None;
        self.finder.input.clear();
        self.finder_update();
    }

    /// Fuzzy finder over recently opened files.
    pub fn open_recent(&mut self) {
        let current = self
            .source
            .path
            .as_deref()
            .and_then(|p| p.canonicalize().ok());
        let recent: Vec<PathBuf> = self
            .state
            .recent()
            .into_iter()
            .filter(|p| Some(p) != current.as_ref())
            .collect();
        if recent.is_empty() {
            self.notify(if self.remember {
                "No recent files"
            } else {
                "No recent files (remember_position is off)"
            });
            return;
        }
        self.mode = Mode::Finder;
        self.finder.recent = Some(recent);
        self.finder.input.clear();
        self.finder_update();
    }

    /// Paths the finder lists.
    pub fn finder_items(&self) -> &[PathBuf] {
        self.finder.recent.as_deref().unwrap_or(&self.files)
    }

    pub fn finder_update(&mut self) {
        self.finder.results = files::fuzzy(self.finder_items(), &self.finder.input.text);
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
        let Some(&idx) = self.finder.results.get(self.finder.selected) else {
            return;
        };
        match self.finder.recent.take() {
            Some(recent) => match Source::from_file(&recent[idx]) {
                Ok(src) => {
                    self.open_source(src);
                    self.notify(format!("Opened {}", recent[idx].display()));
                }
                Err(e) => self.notify(format!("{e:#}")),
            },
            None => {
                self.open_file(idx);
            }
        }
    }
}
