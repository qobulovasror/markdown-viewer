//! Searching every document of the file list / folder (`S`).

use super::app::{App, Mode};
use super::input::LineInput;
use crate::search;

/// Most hits collected for one query.
const MAX_HITS: usize = 500;
/// Larger files are skipped.
const MAX_FILE_BYTES: u64 = 5 << 20;
/// Reading stops after this much text, so huge folders don't stall the UI.
const MAX_TOTAL_BYTES: u64 = 100 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Index into `App::files`.
    pub file: usize,
    /// 1-based source line.
    pub line: usize,
    /// Which match of the query this is within the file (0-based).
    pub nth: usize,
    pub text: String,
}

#[derive(Debug, Default)]
pub struct Grep {
    pub input: LineInput,
    /// Query the hits belong to.
    pub ran: String,
    pub hits: Vec<Hit>,
    pub selected: usize,
    /// Hit limit was reached.
    pub truncated: bool,
}

impl App {
    pub fn open_grep(&mut self) {
        if !self.ensure_files() {
            return;
        }
        self.mode = Mode::Grep;
        if !self.search.query.is_empty() && self.grep.input.is_empty() {
            let q = self.search.query.clone();
            self.grep.input.set(&q);
        }
    }

    /// Enter: runs a new query, or opens the selected hit.
    pub fn grep_enter(&mut self) {
        if self.grep.input.text != self.grep.ran || self.grep.hits.is_empty() {
            self.grep_run();
            return;
        }
        let Some(hit) = self.grep.hits.get(self.grep.selected).cloned() else {
            return;
        };
        self.mode = Mode::Normal;
        if !self.open_file(hit.file) {
            return;
        }
        let q = self.grep.ran.clone();
        self.search_for(&q, hit.nth);
        // Rendered text drops markup and adds labels, so the nth rendered match may
        // be off; prefer a nearby match on a line that looks like the hit's line.
        let key = letters(&hit.text);
        let best = self
            .search
            .matches
            .iter()
            .enumerate()
            .filter(|(_, m)| {
                let l = letters(&self.rendered.lines[m.line].text());
                !l.is_empty() && (key.contains(&l) || l.contains(&key))
            })
            .min_by_key(|(i, _)| i.abs_diff(hit.nth))
            .map(|(i, _)| i);
        if let Some(i) = best {
            self.search_for(&q, i);
        }
    }

    fn grep_run(&mut self) {
        let query = self.grep.input.text.clone();
        self.grep.ran = query.clone();
        self.grep.hits.clear();
        self.grep.selected = 0;
        self.grep.truncated = false;
        let Some(re) = search::compile(&query) else {
            return;
        };
        let root = self.root.clone().unwrap_or_default();
        let mut total = 0;
        'files: for (i, rel) in self.files.iter().enumerate() {
            let path = root.join(rel);
            let size = std::fs::metadata(&path).map_or(0, |m| m.len());
            if size > MAX_FILE_BYTES {
                continue;
            }
            total += size;
            if total > MAX_TOTAL_BYTES {
                self.grep.truncated = true;
                break;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            let mut nth = 0;
            let skip = front_matter_lines(&text);
            for (n, line) in text.lines().enumerate().skip(skip) {
                let count = re.find_iter(line).filter(|m| !m.is_empty()).count();
                if count == 0 {
                    continue;
                }
                if self.grep.hits.len() == MAX_HITS {
                    self.grep.truncated = true;
                    break 'files;
                }
                self.grep.hits.push(Hit {
                    file: i,
                    line: n + 1,
                    nth,
                    text: line.trim().chars().take(200).collect(),
                });
                nth += count;
            }
        }
    }

    pub fn grep_move(&mut self, delta: isize) {
        let n = self.grep.hits.len();
        if n > 0 {
            self.grep.selected = self.grep.selected.saturating_add_signed(delta).min(n - 1);
        }
    }
}

/// Lowercase letters and digits only, to compare source and rendered lines.
fn letters(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Lines of a leading `---` … `---` block, which the viewer does not render.
fn front_matter_lines(text: &str) -> usize {
    let mut lines = text.lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return 0;
    }
    lines
        .position(|l| matches!(l.trim_end(), "---" | "..."))
        .map_or(0, |end| end + 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_front_matter() {
        assert_eq!(front_matter_lines("---\ntitle: x\n---\nbody"), 3);
        assert_eq!(front_matter_lines("# no\n---\n"), 0);
        assert_eq!(front_matter_lines("---\nnever closed"), 0);
        assert_eq!(letters("**Bold** `x`!"), "boldx");
    }
}
