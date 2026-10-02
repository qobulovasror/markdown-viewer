//! Viewer state and document-level operations.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::color::ColorDepth;
use crate::config::Config;
use crate::layout::{self, Rendered};
use crate::links::{self, Target};
use crate::parser::{self, Document};
use crate::search::{self, Match};
use crate::state::{Position, State};
use crate::watch::FileWatcher;
use crate::theme::Theme;

/// Where the document text came from.
#[derive(Debug, Clone)]
pub struct Source {
    pub path: Option<PathBuf>,
    pub text: String,
    pub format: parser::Format,
}

impl Source {
    pub fn from_file(path: &Path) -> Result<Source> {
        let bytes =
            std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        Ok(Source {
            path: Some(path.to_path_buf()),
            text: String::from_utf8_lossy(&bytes).into_owned(),
            format: parser::Format::from_path(path),
        })
    }

    pub fn name(&self) -> String {
        match &self.path {
            Some(p) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            None => "stdin".into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Help,
    /// Typing a search query.
    Search,
    /// Document info and front matter popup.
    Info,
    /// Fuzzy file finder.
    Finder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Content,
    Panel,
}

/// Side panel content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Toc,
    Files,
}

#[derive(Debug, Default)]
pub struct SearchState {
    pub input: String,
    pub query: String,
    pub matches: Vec<Match>,
    pub current: Option<usize>,
    /// Scroll position before an interactive search started.
    origin: usize,
}

/// A link occurrence: link id plus the first line it appears on.
#[derive(Debug, Clone, Copy)]
pub struct LinkRef {
    pub id: usize,
    pub line: usize,
}

/// Key prefix waiting for its argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pending {
    Mark,
    Jump,
    Yank,
}

pub(super) struct HistoryEntry {
    pub(super) source: Source,
    pub(super) scroll: usize,
}

pub struct App {
    pub theme: Theme,
    pub depth: ColorDepth,
    pub source: Source,
    pub doc: Document,
    pub rendered: Rendered,
    pub(super) layout_width: usize,
    pub scroll: usize,
    pub view_height: usize,
    pub mode: Mode,
    pub mouse: bool,
    /// Maximum text width (focus mode); 0 means full width.
    pub max_width: usize,
    pub words: usize,
    pub panel: Option<Panel>,
    pub focus: Focus,
    pub toc_selected: usize,
    pub search: SearchState,
    pub link_refs: Vec<LinkRef>,
    pub link_focus: Option<usize>,
    pub(super) history: Vec<HistoryEntry>,
    /// Screen areas from the last draw, for mouse hit-testing.
    pub text_rect: ratatui::layout::Rect,
    pub panel_rect: Option<ratatui::layout::Rect>,
    /// Directory mode root and its documents (relative paths).
    pub root: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub files_selected: usize,
    pub finder: super::files_panel::Finder,
    pub images: Option<super::images::Images>,
    pub pending: Option<Pending>,
    /// Width restored when leaving full-width mode.
    pub focus_width: usize,
    pub(super) state: State,
    pub(super) remember: bool,
    /// Position to restore after the first layout.
    pending_restore: Option<Position>,
    pub(super) watcher: Option<FileWatcher>,
    /// Set by the `e` key; the event loop suspends the UI and runs the editor.
    pub open_editor: bool,
    message: Option<(String, Instant)>,
    pub quit: bool,
}

impl App {
    pub fn new(source: Source, theme: Theme, depth: ColorDepth, cfg: &Config) -> App {
        let doc = parser::parse(&source.text, source.format);
        let words = count_words(&source.text);
        let state = if cfg.remember_position {
            State::load()
        } else {
            State::default()
        };
        let pending_restore = source
            .path
            .as_deref()
            .and_then(|p| state.get(p))
            .map(|f| f.position.clone());
        let mut app = App {
            theme,
            depth,
            source,
            doc,
            rendered: Rendered::default(),
            layout_width: 0,
            scroll: 0,
            view_height: 1,
            mode: Mode::Normal,
            mouse: cfg.mouse,
            max_width: cfg.max_width,
            words,
            panel: cfg.toc.then_some(Panel::Toc),
            focus: Focus::Content,
            toc_selected: 0,
            search: SearchState::default(),
            link_refs: Vec::new(),
            link_focus: None,
            history: Vec::new(),
            text_rect: Default::default(),
            panel_rect: None,
            root: None,
            files: Vec::new(),
            files_selected: 0,
            finder: Default::default(),
            images: None,
            pending: None,
            focus_width: if cfg.max_width > 0 { cfg.max_width } else { 100 },
            state,
            remember: cfg.remember_position,
            pending_restore,
            watcher: None,
            open_editor: false,
            message: None,
            quit: false,
        };
        if cfg.watch {
            app.toggle_watch();
        }
        app
    }

    /// Re-lays out the document if the width changed, keeping the reading position.
    pub fn ensure_layout(&mut self, width: usize) {
        if width == self.layout_width && !self.rendered.lines.is_empty() {
            return;
        }
        let anchor = self.position_anchor();
        self.relayout(width);
        match self.pending_restore.take() {
            Some(pos) => self.scroll_to(self.line_of(&pos)),
            None => self.restore_anchor(anchor),
        }
    }

    pub(super) fn relayout(&mut self, width: usize) {
        self.layout_width = width;
        let image_rows = match &mut self.images {
            Some(img) => img.plan(&self.doc, self.source.path.as_deref(), width as u16),
            None => Default::default(),
        };
        self.rendered = layout::layout(
            &self.doc,
            &self.theme,
            &layout::Options {
                width,
                code_numbers: true,
                front_matter: false,
                image_rows,
            },
        );
        self.link_refs = collect_link_refs(&self.rendered);
        self.link_focus = None;
        self.refresh_search();
    }

    /// Current position as (heading index, lines below it) or a fraction.
    pub(super) fn position_anchor(&self) -> (Option<usize>, usize, f64) {
        let total = self.rendered.lines.len().max(1);
        let frac = self.scroll as f64 / total as f64;
        match self.current_heading() {
            Some(h) => {
                let line = self.rendered.headings[h].line;
                (Some(h), self.scroll.saturating_sub(line), frac)
            }
            None => (None, self.scroll, frac),
        }
    }

    pub(super) fn restore_anchor(&mut self, (heading, offset, frac): (Option<usize>, usize, f64)) {
        self.scroll = match heading.and_then(|h| self.rendered.headings.get(h)) {
            Some(h) => h.line + offset,
            None if self.scroll == 0 => 0,
            None => (frac * self.rendered.lines.len() as f64) as usize,
        };
        self.clamp_scroll();
    }

    pub fn reload(&mut self) -> Result<()> {
        let Some(path) = self.source.path.clone() else {
            self.notify("stdin cannot be reloaded");
            return Ok(());
        };
        let format = self.source.format;
        self.source = Source::from_file(&path)?;
        self.source.format = format;
        self.reparse();
        let anchor = self.position_anchor();
        self.relayout(self.layout_width.max(20));
        self.restore_anchor(anchor);
        Ok(())
    }

    fn reparse(&mut self) {
        self.doc = parser::parse(&self.source.text, self.source.format);
        self.words = count_words(&self.source.text);
    }

    /// Replaces the current document, remembering it in the history.
    pub(super) fn open_source(&mut self, source: Source) {
        self.save_state();
        let prev = std::mem::replace(&mut self.source, source);
        self.history.push(HistoryEntry {
            source: prev,
            scroll: self.scroll,
        });
        self.reparse();
        self.relayout(self.layout_width.max(20));
        self.scroll = 0;
        self.toc_selected = 0;
        self.rewatch();
    }

    pub fn back(&mut self) {
        let Some(entry) = self.history.pop() else {
            self.notify("No previous document");
            return;
        };
        let same = entry.source.path == self.source.path && entry.source.text == self.source.text;
        if !same {
            self.save_state();
            self.source = entry.source;
            self.reparse();
            self.relayout(self.layout_width.max(20));
            self.rewatch();
        }
        self.scroll_to(entry.scroll);
    }

    /// Jumps to an anchor in the current document, recording the old position.
    pub fn jump_to_anchor(&mut self, id: &str) -> bool {
        self.jump_to_anchor_inner(id, true)
    }

    fn jump_to_anchor_inner(&mut self, id: &str, record: bool) -> bool {
        let target = self
            .rendered
            .anchor_line(id)
            .or_else(|| self.rendered.anchor_line(&parser::markdown::slugify(id)));
        match target {
            Some(line) => {
                if record {
                    self.history.push(HistoryEntry {
                        source: self.source.clone(),
                        scroll: self.scroll,
                    });
                }
                self.scroll_to(line);
                true
            }
            None => {
                self.notify(format!("Anchor #{id} not found"));
                false
            }
        }
    }

    pub fn follow_link(&mut self, id: usize) {
        let Some(url) = self.rendered.links.get(id).cloned() else {
            return;
        };
        match links::resolve(&url, self.source.path.as_deref()) {
            Target::Anchor(a) => {
                self.jump_to_anchor(&a);
            }
            Target::Document(path, anchor) => match Source::from_file(&path) {
                Ok(src) => {
                    self.open_source(src);
                    if let Some(a) = anchor {
                        self.jump_to_anchor_inner(&a, false);
                    }
                    self.notify(format!("Opened {} (Backspace to go back)", path.display()));
                }
                Err(e) => self.notify(format!("{e:#}")),
            },
            Target::External(t) => match links::open_external(&t) {
                Ok(()) => self.notify(format!("Opening {t}")),
                Err(e) => self.notify(format!("Cannot open {t}: {e}")),
            },
        }
    }

    /// Moves link focus forward/backward, starting from the viewport.
    pub fn cycle_link(&mut self, forward: bool) {
        if self.link_refs.is_empty() {
            self.notify("No links");
            return;
        }
        let n = self.link_refs.len();
        let visible = |r: &LinkRef| r.line >= self.scroll && r.line < self.scroll + self.view_height;
        let next = match self.link_focus {
            Some(i) if visible(&self.link_refs[i]) => {
                if forward {
                    (i + 1) % n
                } else {
                    (i + n - 1) % n
                }
            }
            _ => {
                let first = self.link_refs.iter().position(|r| r.line >= self.scroll);
                match (forward, first) {
                    (true, Some(i)) => i,
                    (true, None) => 0,
                    (false, Some(i)) => {
                        let last = self.link_refs.iter().rposition(visible);
                        last.unwrap_or((i + n - 1) % n)
                    }
                    (false, None) => n - 1,
                }
            }
        };
        self.link_focus = Some(next);
        self.ensure_visible(self.link_refs[next].line);
    }

    pub fn focused_link_id(&self) -> Option<usize> {
        self.link_focus.map(|i| self.link_refs[i].id)
    }

    pub fn ensure_visible(&mut self, line: usize) {
        if line < self.scroll || line >= self.scroll + self.view_height {
            self.scroll_to(line.saturating_sub(self.view_height / 3));
        }
    }

    /// Jumps to the next/previous heading relative to the viewport top.
    pub fn jump_heading(&mut self, forward: bool) {
        let hs = &self.rendered.headings;
        let target = if forward {
            hs.iter().find(|h| h.line > self.scroll)
        } else {
            hs.iter().rev().find(|h| h.line < self.scroll)
        };
        if let Some(h) = target {
            let line = h.line;
            self.scroll_to(line);
        }
    }

    pub fn start_search(&mut self) {
        self.mode = Mode::Search;
        self.search.input.clear();
        self.search.origin = self.scroll;
    }

    /// Live update while typing.
    pub fn update_search_input(&mut self) {
        let input = self.search.input.clone();
        self.set_query(&input);
        if self.search.current.is_none() {
            self.scroll_to(self.search.origin);
        }
    }

    pub fn finish_search(&mut self, accept: bool) {
        self.mode = Mode::Normal;
        if !accept {
            self.set_query("");
            self.scroll_to(self.search.origin);
        } else if self.search.matches.is_empty() && !self.search.query.is_empty() {
            self.notify(format!("Pattern not found: {}", self.search.query));
        }
    }

    pub fn clear_search(&mut self) {
        self.set_query("");
    }

    fn set_query(&mut self, query: &str) {
        self.search.query = query.to_string();
        self.refresh_search();
        // Pick the first match at or after the search origin.
        let from = self.search.origin;
        self.search.current = self
            .search
            .matches
            .iter()
            .position(|m| m.line >= from)
            .or((!self.search.matches.is_empty()).then_some(0));
        if let Some(i) = self.search.current {
            self.ensure_visible(self.search.matches[i].line);
        }
    }

    fn refresh_search(&mut self) {
        self.search.matches = match search::compile(&self.search.query) {
            Some(re) => search::find_all(&self.rendered.lines, &re),
            None => Vec::new(),
        };
        let n = self.search.matches.len();
        self.search.current = self.search.current.filter(|&c| c < n);
    }

    pub fn next_match(&mut self, forward: bool) {
        let n = self.search.matches.len();
        if n == 0 {
            if !self.search.query.is_empty() {
                self.notify(format!("Pattern not found: {}", self.search.query));
            }
            return;
        }
        let cur = self.search.current.unwrap_or(0);
        let next = if forward { (cur + 1) % n } else { (cur + n - 1) % n };
        if (forward && next < cur) || (!forward && next > cur) {
            self.notify("Search wrapped");
        }
        self.search.current = Some(next);
        self.ensure_visible(self.search.matches[next].line);
    }

    pub fn toggle_toc(&mut self) {
        if self.rendered.headings.is_empty() {
            self.notify("No headings");
            return;
        }
        if self.panel == Some(Panel::Toc) {
            self.panel = None;
            self.focus = Focus::Content;
        } else {
            self.panel = Some(Panel::Toc);
            self.focus = Focus::Panel;
            self.toc_selected = self.current_heading().unwrap_or(0);
        }
    }

    /// TOC entry to highlight: selection when focused, else current section.
    pub fn toc_highlight(&self) -> usize {
        if self.focus == Focus::Panel {
            self.toc_selected
        } else {
            self.current_heading().unwrap_or(0)
        }
    }

    /// First visible TOC row for a panel of `height` rows.
    pub fn toc_offset(&self, height: usize) -> usize {
        let n = self.rendered.headings.len();
        self.toc_highlight()
            .saturating_sub(height / 2)
            .min(n.saturating_sub(height))
    }

    pub fn toc_move(&mut self, delta: isize) {
        let n = self.rendered.headings.len();
        if n > 0 {
            self.toc_selected = self.toc_selected.saturating_add_signed(delta).min(n - 1);
        }
    }

    pub fn toc_jump(&mut self, idx: usize) {
        if let Some(h) = self.rendered.headings.get(idx) {
            let id = h.id.clone();
            self.toc_selected = idx;
            self.jump_to_anchor(&id);
        }
    }

    /// Switches to the next built-in theme.
    pub fn cycle_theme(&mut self) {
        let names = crate::theme::THEME_NAMES;
        let i = names.iter().position(|n| *n == self.theme.name).unwrap_or(0);
        let name = names[(i + 1) % names.len()];
        self.theme = Theme::by_name(name).expect("built-in theme");
        let anchor = self.position_anchor();
        self.relayout(self.layout_width.max(20));
        self.restore_anchor(anchor);
        self.notify(format!("Theme: {name}"));
    }

    pub fn max_scroll(&self) -> usize {
        self.rendered.lines.len().saturating_sub(self.view_height)
    }

    pub fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }

    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll = self.scroll.saturating_add_signed(delta);
        self.clamp_scroll();
    }

    pub fn scroll_to(&mut self, line: usize) {
        self.scroll = line;
        self.clamp_scroll();
    }

    /// Index of the last heading at or above the top visible line.
    pub fn current_heading(&self) -> Option<usize> {
        self.rendered
            .headings
            .iter()
            .rposition(|h| h.line <= self.scroll)
    }

    pub fn progress(&self) -> u16 {
        let max = self.max_scroll();
        if max == 0 {
            100
        } else {
            (self.scroll * 100 / max) as u16
        }
    }

    /// Estimated reading time in minutes (200 words per minute).
    pub fn reading_minutes(&self) -> usize {
        self.words.div_ceil(200).max(1)
    }

    pub fn notify(&mut self, msg: impl Into<String>) {
        self.message = Some((msg.into(), Instant::now()));
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_ref().map(|(m, _)| m.as_str())
    }

    /// Periodic housekeeping between events.
    pub fn tick(&mut self) {
        if self.watcher.as_ref().is_some_and(FileWatcher::changed) {
            match self.reload() {
                Ok(()) => self.notify("File changed, reloaded"),
                Err(e) => self.notify(format!("Reload failed: {e:#}")),
            }
        }
        if self
            .message
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(3))
        {
            self.message = None;
        }
    }
}

fn collect_link_refs(r: &Rendered) -> Vec<LinkRef> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (n, line) in r.lines.iter().enumerate() {
        for s in &line.spans {
            if let Some(id) = s.link
                && seen.insert(id)
            {
                out.push(LinkRef { id, line: n });
            }
        }
    }
    out
}

fn count_words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}
