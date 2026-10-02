//! Viewer state and document-level operations.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::color::ColorDepth;
use crate::layout::{self, Rendered};
use crate::parser::{self, Document};
use crate::theme::Theme;

/// Where the document text came from.
#[derive(Debug, Clone)]
pub struct Source {
    pub path: Option<PathBuf>,
    pub text: String,
}

impl Source {
    pub fn from_file(path: &Path) -> Result<Source> {
        let bytes =
            std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        Ok(Source {
            path: Some(path.to_path_buf()),
            text: String::from_utf8_lossy(&bytes).into_owned(),
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
}

pub struct App {
    pub theme: Theme,
    pub depth: ColorDepth,
    pub source: Source,
    pub doc: Document,
    pub rendered: Rendered,
    layout_width: usize,
    pub scroll: usize,
    pub view_height: usize,
    pub mode: Mode,
    pub mouse: bool,
    /// Maximum text width (focus mode); 0 means full width.
    pub max_width: usize,
    pub words: usize,
    message: Option<(String, Instant)>,
    pub quit: bool,
}

impl App {
    pub fn new(source: Source, theme: Theme, depth: ColorDepth, max_width: usize) -> App {
        let doc = parser::markdown::parse(&source.text);
        let words = count_words(&source.text);
        App {
            theme,
            depth,
            source,
            doc,
            rendered: Rendered::default(),
            layout_width: 0,
            scroll: 0,
            view_height: 1,
            mode: Mode::Normal,
            mouse: true,
            max_width,
            words,
            message: None,
            quit: false,
        }
    }

    /// Re-lays out the document if the width changed, keeping the reading position.
    pub fn ensure_layout(&mut self, width: usize) {
        if width == self.layout_width && !self.rendered.lines.is_empty() {
            return;
        }
        let anchor = self.position_anchor();
        self.relayout(width);
        self.restore_anchor(anchor);
    }

    fn relayout(&mut self, width: usize) {
        self.layout_width = width;
        self.rendered = layout::layout(
            &self.doc,
            &self.theme,
            &layout::Options {
                width,
                code_numbers: true,
            },
        );
    }

    /// Current position as (heading index, lines below it) or a fraction.
    fn position_anchor(&self) -> (Option<usize>, usize, f64) {
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

    fn restore_anchor(&mut self, (heading, offset, frac): (Option<usize>, usize, f64)) {
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
        self.source = Source::from_file(&path)?;
        self.doc = parser::markdown::parse(&self.source.text);
        self.words = count_words(&self.source.text);
        let anchor = self.position_anchor();
        self.relayout(self.layout_width.max(20));
        self.restore_anchor(anchor);
        Ok(())
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
        if self
            .message
            .as_ref()
            .is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(3))
        {
            self.message = None;
        }
    }
}

fn count_words(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .count()
}
