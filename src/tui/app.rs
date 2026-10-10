//! Viewer state and document-level operations.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use unicode_width::UnicodeWidthStr;

use super::input::{History, LineInput};
use super::keymap::Keymap;
use crate::color::ColorDepth;
use crate::config::Config;
use crate::layout::{self, Rendered};
use crate::links::{self, Target};
use crate::parser::{self, Document};
use crate::search::{self, Match};
use crate::state::{Position, State};
use crate::theme::Theme;
use crate::watch::FileWatcher;

/// Where the document text came from.
#[derive(Debug, Clone)]
pub struct Source {
    pub path: Option<PathBuf>,
    /// Set for documents fetched over http(s); `path` is then `None`.
    pub url: Option<String>,
    pub text: String,
    pub format: parser::Format,
}

impl Source {
    pub fn from_file(path: &Path) -> Result<Source> {
        let bytes =
            std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        Ok(Source {
            path: Some(path.to_path_buf()),
            url: None,
            text: String::from_utf8_lossy(&bytes).into_owned(),
            format: parser::Format::from_path(path),
        })
    }

    /// Downloads a URL or GitHub shorthand (`gh:owner/repo`).
    #[cfg(feature = "remote")]
    pub fn from_url(input: &str) -> Result<Source> {
        let (url, text) = crate::remote::fetch(input)?;
        let name = crate::remote::file_name(&url);
        Ok(Source {
            path: None,
            format: parser::Format::from_path(Path::new(&name)),
            url: Some(url),
            text,
        })
    }

    #[cfg(not(feature = "remote"))]
    pub fn from_url(input: &str) -> Result<Source> {
        anyhow::bail!("cannot open {input}: mdvw was built without the `remote` feature")
    }

    pub fn name(&self) -> String {
        match (&self.path, &self.url) {
            (Some(p), _) => p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.display().to_string()),
            #[cfg(feature = "remote")]
            (None, Some(u)) => crate::remote::file_name(u),
            _ => "stdin".into(),
        }
    }

    /// Full location for messages: path, URL or `stdin`.
    pub fn location(&self) -> String {
        match (&self.path, &self.url) {
            (Some(p), _) => p.display().to_string(),
            (None, Some(u)) => u.clone(),
            _ => "stdin".into(),
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
    /// Search across the file list / folder.
    Grep,
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
    pub input: LineInput,
    pub history: History,
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
    /// Waiting for `y` to open `App::confirm_target`.
    Open,
}

/// A download to run after the next draw.
pub struct Fetch {
    pub url: String,
    pub anchor: Option<String>,
    /// Replace the current document instead of opening a new one.
    pub reload: bool,
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
    /// Position in `toc_entries()`.
    pub toc_selected: usize,
    /// Narrows the contents panel to matching headings.
    pub toc_filter: LineInput,
    /// Typing into `toc_filter`.
    pub toc_filtering: bool,
    pub search: SearchState,
    pub link_refs: Vec<LinkRef>,
    pub link_focus: Option<usize>,
    pub(super) history: Vec<HistoryEntry>,
    /// Entries undone by `back`, replayed by `forward`.
    pub(super) forward: Vec<HistoryEntry>,
    pub keymap: Keymap,
    /// Numeric prefix typed before a command (`5j`, `50%`).
    pub count: Option<usize>,
    pub help_scroll: usize,
    pub fetch: Option<Fetch>,
    pub wrap_code: bool,
    /// Columns that unwrapped code rows are scrolled to the left.
    pub hscroll: usize,
    /// Largest useful `hscroll` for the current layout.
    pub(super) hscroll_max: usize,
    /// Screen areas from the last draw, for mouse hit-testing.
    pub text_rect: ratatui::layout::Rect,
    pub panel_rect: Option<ratatui::layout::Rect>,
    /// Directory mode root and its documents (relative paths).
    pub root: Option<PathBuf>,
    pub files: Vec<PathBuf>,
    pub files_selected: usize,
    pub finder: super::files_panel::Finder,
    pub grep: super::grep::Grep,
    pub images: Option<super::images::Images>,
    pub pending: Option<Pending>,
    /// External target waiting for confirmation (`Pending::Open`).
    pub confirm_target: Option<String>,
    /// Width restored when leaving full-width mode.
    pub focus_width: usize,
    pub(super) state: State,
    pub(super) remember: bool,
    /// Position to restore after the first layout.
    pending_restore: Option<Position>,
    /// Heading to show after the first layout (`file.md#anchor`).
    start_anchor: Option<String>,
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
        let (keymap, key_warnings) = Keymap::new(&cfg.keys);
        let warnings: Vec<String> = cfg.warnings.iter().chain(&key_warnings).cloned().collect();
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
            toc_filter: LineInput::default(),
            toc_filtering: false,
            search: SearchState::default(),
            link_refs: Vec::new(),
            link_focus: None,
            history: Vec::new(),
            forward: Vec::new(),
            keymap,
            count: None,
            help_scroll: 0,
            fetch: None,
            wrap_code: cfg.wrap_code,
            hscroll: 0,
            hscroll_max: 0,
            text_rect: Default::default(),
            panel_rect: None,
            root: None,
            files: Vec::new(),
            files_selected: 0,
            finder: Default::default(),
            grep: Default::default(),
            images: None,
            pending: None,
            confirm_target: None,
            focus_width: if cfg.max_width > 0 {
                cfg.max_width
            } else {
                100
            },
            state,
            remember: cfg.remember_position,
            pending_restore,
            start_anchor: None,
            watcher: None,
            open_editor: false,
            message: None,
            quit: false,
        };
        if cfg.watch {
            app.toggle_watch();
        }
        if !warnings.is_empty() {
            app.notify(format!("Config: {}", warnings.join("; ")));
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
        if let Some(a) = self.start_anchor.take() {
            self.pending_restore = None;
            self.jump_to_anchor_inner(&a, false);
            return;
        }
        match self.pending_restore.take() {
            Some(pos) => self.scroll_to(self.line_of(&pos)),
            None => self.restore_anchor(anchor),
        }
    }

    pub(super) fn relayout(&mut self, width: usize) {
        self.layout_width = width;
        // Relative images of a fetched document would resolve against the cwd.
        let image_rows = match &mut self.images {
            Some(_) if self.source.url.is_some() => Default::default(),
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
                wrap_code: self.wrap_code,
            },
        );
        self.link_refs = collect_link_refs(&self.rendered);
        self.hscroll_max = self
            .rendered
            .lines
            .iter()
            .filter_map(|l| {
                let inner = l.hscroll? as usize;
                Some(crate::layout::wrap::width(&l.spans).saturating_sub(4 + inner))
            })
            .max()
            .unwrap_or(0);
        self.hscroll = self.hscroll.min(self.hscroll_max);
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
        let format = self.source.format;
        let fresh = match (self.source.path.clone(), self.source.url.clone()) {
            (Some(path), _) => Source::from_file(&path)?,
            (None, Some(url)) => {
                self.notify(format!("Loading {url}…"));
                self.fetch = Some(Fetch {
                    url,
                    anchor: None,
                    reload: true,
                });
                return Ok(());
            }
            _ => {
                self.notify("stdin cannot be reloaded");
                return Ok(());
            }
        };
        self.cancel_pending();
        self.source = fresh;
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
        self.cancel_pending();
        self.save_state();
        let prev = std::mem::replace(&mut self.source, source);
        self.history.push(HistoryEntry {
            source: prev,
            scroll: self.scroll,
        });
        self.forward.clear();
        self.hscroll = 0;
        self.reparse();
        self.relayout(self.layout_width.max(20));
        self.scroll = 0;
        self.toc_selected = 0;
        self.toc_filter.clear();
        self.toc_filtering = false;
        self.rewatch();
    }

    pub fn start_at_anchor(&mut self, anchor: String) {
        self.start_anchor = Some(anchor);
    }

    /// Saves the current place so `back` can return to it.
    pub(super) fn record(&mut self) {
        self.history.push(HistoryEntry {
            source: self.source.clone(),
            scroll: self.scroll,
        });
        self.forward.clear();
    }

    pub fn back(&mut self) {
        let Some(entry) = self.history.pop() else {
            self.notify("No previous location");
            return;
        };
        self.forward.push(HistoryEntry {
            source: self.source.clone(),
            scroll: self.scroll,
        });
        self.restore_entry(entry);
    }

    pub fn forward(&mut self) {
        let Some(entry) = self.forward.pop() else {
            self.notify("No next location");
            return;
        };
        self.history.push(HistoryEntry {
            source: self.source.clone(),
            scroll: self.scroll,
        });
        self.restore_entry(entry);
    }

    fn restore_entry(&mut self, entry: HistoryEntry) {
        let mut source = entry.source;
        // Files may have changed since; show their current text.
        if let Some(path) = source.path.clone()
            && let Ok(fresh) = Source::from_file(&path)
        {
            source.text = fresh.text;
        }
        let same = source.path == self.source.path
            && source.url == self.source.url
            && source.text == self.source.text;
        if !same {
            self.save_state();
            self.toc_filter.clear();
            self.toc_filtering = false;
            self.toc_selected = 0;
            self.hscroll = 0;
            self.source = source;
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
                    self.record();
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

    /// Where a link points; links in a fetched document resolve against its URL.
    pub(super) fn link_target(&self, url: &str) -> Target {
        match &self.source.url {
            Some(base) if !url.starts_with('#') => {
                #[cfg(feature = "remote")]
                if let Some(abs) = crate::remote::join(base, url) {
                    return Target::External(abs);
                }
                let _ = base;
                Target::External(url.to_string())
            }
            _ => links::resolve(url, self.source.path.as_deref()),
        }
    }

    /// Opens a link: documents in the viewer (local or web), the rest externally.
    pub fn follow_link(&mut self, id: usize) {
        self.follow(id, false);
    }

    /// Opens a link with the system handler even if mdvw could show it.
    pub fn open_link_externally(&mut self, id: usize) {
        self.follow(id, true);
    }

    fn follow(&mut self, id: usize, external: bool) {
        let Some(url) = self.rendered.links.get(id).cloned() else {
            return;
        };
        let target = match self.link_target(&url) {
            Target::Document(path, _) if external => {
                Target::External(path.to_string_lossy().into_owned())
            }
            t => t,
        };
        #[cfg(feature = "remote")]
        if let Target::External(t) = &target
            && !external
            && let Some((doc, anchor)) = crate::remote::document_url(t)
        {
            self.notify(format!("Loading {doc}…"));
            self.fetch = Some(Fetch {
                url: doc,
                anchor,
                reload: false,
            });
            return;
        }
        match target {
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
            Target::External(t) if links::needs_confirm(&t) => {
                self.notify(format!("Open {t} with the system handler? (y/N)"));
                self.confirm_target = Some(t);
                self.pending = Some(Pending::Open);
            }
            Target::External(t) => self.open_external(&t),
        }
    }

    /// Runs a download requested by `follow` or `reload`; the event loop calls
    /// this after drawing, so the "Loading…" message is visible meanwhile.
    pub fn run_fetch(&mut self) {
        let Some(f) = self.fetch.take() else {
            return;
        };
        match Source::from_url(&f.url) {
            Ok(mut src) if f.reload => {
                src.format = self.source.format;
                self.source = src;
                self.reparse();
                let anchor = self.position_anchor();
                self.relayout(self.layout_width.max(20));
                self.restore_anchor(anchor);
                self.notify("Reloaded");
            }
            Ok(src) => {
                self.open_source(src);
                if let Some(a) = f.anchor {
                    self.jump_to_anchor_inner(&a, false);
                }
                self.notify(format!("Opened {} (Backspace to go back)", f.url));
            }
            Err(e) => self.notify(format!("{e:#}")),
        }
    }

    pub fn cancel_pending(&mut self) {
        if self.pending.take().is_some() {
            self.confirm_target = None;
            self.notify("Cancelled");
        }
    }

    pub fn open_external(&mut self, t: &str) {
        match links::open_external(t) {
            Ok(()) => self.notify(format!("Opening {t}")),
            Err(e) => self.notify(format!("Cannot open {t}: {e}")),
        }
    }

    /// Moves link focus forward/backward, starting from the viewport.
    pub fn cycle_link(&mut self, forward: bool) {
        if self.link_refs.is_empty() {
            self.notify("No links");
            return;
        }
        let n = self.link_refs.len();
        let visible =
            |r: &LinkRef| r.line >= self.scroll && r.line < self.scroll + self.view_height;
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
        self.search.history.reset();
        self.search.origin = self.scroll;
    }

    /// Live update while typing.
    pub fn update_search_input(&mut self) {
        let input = self.search.input.text.clone();
        self.set_query(&input);
        if self.search.current.is_none() {
            self.scroll_to(self.search.origin);
        }
    }

    pub fn finish_search(&mut self, accept: bool) {
        self.mode = Mode::Normal;
        if accept {
            self.search.history.push(&self.search.input.text);
        }
        if !accept {
            self.set_query("");
            self.scroll_to(self.search.origin);
        } else if self.search.matches.is_empty() && !self.search.query.is_empty() {
            self.notify(format!("Pattern not found: {}", self.search.query));
        }
    }

    /// Searches for `query` and selects its `nth` match (clamped), as after a
    /// folder search hit; source and rendered text may differ slightly.
    pub fn search_for(&mut self, query: &str, nth: usize) {
        self.search.query = query.to_string();
        self.refresh_search();
        let n = self.search.matches.len();
        if n == 0 {
            self.notify(format!("Pattern not found: {query}"));
            return;
        }
        let i = nth.min(n - 1);
        self.search.current = Some(i);
        self.reveal_match(i);
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
            self.reveal_match(i);
        }
    }

    /// Scrolls so match `i` is on screen, sideways too for unwrapped code rows.
    fn reveal_match(&mut self, i: usize) {
        let m = self.search.matches[i];
        self.ensure_visible(m.line);
        let line = &self.rendered.lines[m.line];
        let Some(inner) = line.hscroll.map(usize::from) else {
            return;
        };
        let text = line.text();
        let start = text
            .get(..m.start)
            .map_or(0, |s| s.width())
            .saturating_sub(2);
        let end = text.get(..m.end).map_or(0, |s| s.width()).saturating_sub(2);
        if start < self.hscroll || end > self.hscroll + inner {
            self.hscroll = start.saturating_sub(inner / 3).min(self.hscroll_max);
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
        let next = if forward {
            (cur + 1) % n
        } else {
            (cur + n - 1) % n
        };
        if (forward && next < cur) || (!forward && next > cur) {
            self.notify("Search wrapped");
        }
        self.search.current = Some(next);
        self.reveal_match(next);
    }

    pub fn toggle_toc(&mut self) {
        if self.rendered.headings.is_empty() {
            self.notify("No headings");
            return;
        }
        self.toc_filtering = false;
        if self.panel == Some(Panel::Toc) {
            self.panel = None;
            self.focus = Focus::Content;
        } else {
            self.panel = Some(Panel::Toc);
            self.focus = Focus::Panel;
            self.toc_selected = self.toc_current().unwrap_or(0);
        }
    }

    /// Headings shown in the contents panel (all, or those matching the filter).
    pub fn toc_entries(&self) -> Vec<usize> {
        let q = self.toc_filter.text.trim();
        let hs = &self.rendered.headings;
        if q.is_empty() {
            return (0..hs.len()).collect();
        }
        // Every word must appear; smart-case like search.
        let sensitive = q.chars().any(char::is_uppercase);
        let fold = |s: &str| {
            if sensitive {
                s.to_string()
            } else {
                s.to_lowercase()
            }
        };
        let words: Vec<String> = q.split_whitespace().map(fold).collect();
        (0..hs.len())
            .filter(|&i| {
                let title = fold(&hs[i].title);
                words.iter().all(|w| title.contains(w.as_str()))
            })
            .collect()
    }

    /// Position of the current section in `toc_entries()`.
    fn toc_current(&self) -> Option<usize> {
        let h = self.current_heading()?;
        self.toc_entries().iter().position(|&i| i == h)
    }

    /// Row to highlight: the selection when focused, else the current section.
    pub fn toc_highlight(&self) -> usize {
        if self.focus == Focus::Panel {
            self.toc_selected
        } else {
            self.toc_current().unwrap_or(0)
        }
    }

    /// First visible TOC row for a panel of `height` rows.
    pub fn toc_offset(&self, height: usize) -> usize {
        let n = self.toc_entries().len();
        self.toc_highlight()
            .saturating_sub(height / 2)
            .min(n.saturating_sub(height))
    }

    pub fn toc_move(&mut self, delta: isize) {
        let n = self.toc_entries().len();
        self.toc_selected = self
            .toc_selected
            .saturating_add_signed(delta)
            .min(n.saturating_sub(1));
    }

    /// Jumps to the heading at row `idx` of the panel.
    pub fn toc_jump(&mut self, idx: usize) {
        let Some(&h) = self.toc_entries().get(idx) else {
            return;
        };
        let id = self.rendered.headings[h].id.clone();
        self.toc_selected = idx;
        self.jump_to_anchor(&id);
    }

    pub fn toc_filter_changed(&mut self) {
        self.toc_selected = 0;
    }

    /// Wraps long code lines, or keeps them whole for horizontal scrolling.
    pub fn toggle_wrap(&mut self) {
        self.wrap_code = !self.wrap_code;
        self.hscroll = 0;
        let anchor = self.position_anchor();
        self.relayout(self.layout_width.max(20));
        self.restore_anchor(anchor);
        self.notify(if self.wrap_code {
            "Code lines wrap"
        } else {
            "Code lines scroll: ← / →"
        });
    }

    pub fn scroll_horizontal(&mut self, delta: isize) {
        if self.wrap_code {
            self.notify("Code lines wrap; press W to scroll them sideways instead");
            return;
        }
        if self.hscroll_max == 0 {
            self.notify("No long code lines");
        }
        self.hscroll = self
            .hscroll
            .saturating_add_signed(delta)
            .min(self.hscroll_max);
    }

    /// Switches to the next built-in theme.
    pub fn cycle_theme(&mut self) {
        let names = crate::theme::THEME_NAMES;
        let i = names
            .iter()
            .position(|n| *n == self.theme.name)
            .unwrap_or(0);
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

    /// Goes to rendered line `n` (1-based), as with `42G`.
    pub fn goto_line(&mut self, n: usize) {
        self.scroll_to(n.saturating_sub(1));
    }

    pub fn goto_percent(&mut self, p: usize) {
        let line = self.max_scroll() * p.min(100) / 100;
        self.scroll_to(line);
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
        (self.scroll * 100)
            .checked_div(self.max_scroll())
            .map_or(100, |p| p as u16)
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
        // A prompt stays visible while its key is pending.
        if self.pending.is_none()
            && self
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
