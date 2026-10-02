//! Document discovery for directory mode and the fuzzy file finder.

use std::path::{Path, PathBuf};

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher};

use crate::links::is_viewable;

const SKIP_DIRS: &[&str] = &["node_modules", "target", "vendor", "dist", "build", "__pycache__"];
const MAX_FILES: usize = 5000;
const MAX_DEPTH: usize = 8;

/// Viewable documents under `root`, as paths relative to it, sorted.
pub fn scan(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk(root, root, 0, &mut out);
    out.sort_by(|a, b| {
        // Files of a directory before its subdirectories, then by name.
        let (da, db) = (a.parent(), b.parent());
        da.cmp(&db).then_with(|| a.cmp(b))
    });
    out
}

fn walk(root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > MAX_DEPTH || out.len() >= MAX_FILES {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let path = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            if !SKIP_DIRS.contains(&name.as_ref()) {
                walk(root, &path, depth + 1, out);
            }
        } else if is_viewable(&path) {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_path_buf());
            }
            if out.len() >= MAX_FILES {
                return;
            }
        }
    }
}

/// Picks the document to show first in a directory.
pub fn default_file(files: &[PathBuf]) -> Option<usize> {
    let rank = |p: &PathBuf| {
        let top = p.components().count() == 1;
        let name = p
            .file_stem()
            .map(|s| s.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match (top, name.as_str()) {
            (true, "readme") => 0,
            (true, "index") => 1,
            (true, _) => 2,
            _ => 3,
        }
    };
    (0..files.len()).min_by_key(|&i| (rank(&files[i]), i))
}

/// Fuzzy-ranks `files` against `query`; returns indices, best first.
pub fn fuzzy(files: &[PathBuf], query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..files.len()).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let names: Vec<String> = files.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let mut scored: Vec<(u32, usize)> = names
        .iter()
        .enumerate()
        .filter_map(|(i, n)| {
            let mut buf = Vec::new();
            let hay = nucleo_matcher::Utf32Str::new(n, &mut buf);
            pattern.score(hay, &mut matcher).map(|s| (s, i))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_ranks_best_first() {
        let files: Vec<PathBuf> = ["docs/install.md", "README.md", "docs/usage/intro.md"]
            .iter()
            .map(PathBuf::from)
            .collect();
        assert_eq!(fuzzy(&files, "intro")[0], 2);
        assert_eq!(fuzzy(&files, "rdme"), vec![1]);
        assert_eq!(fuzzy(&files, "").len(), 3);
        assert_eq!(default_file(&files), Some(1));
    }

    #[test]
    fn scans_examples() {
        let files = scan(Path::new("examples"));
        assert!(files.contains(&PathBuf::from("sample.md")));
        assert!(files.contains(&PathBuf::from("sample.org")));
    }
}
