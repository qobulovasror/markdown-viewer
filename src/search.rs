//! Text search over rendered lines (smart-case, regex with literal fallback).

use regex::{Regex, RegexBuilder};

use crate::layout::Line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Match {
    pub line: usize,
    /// Byte range within `Line::text()`.
    pub start: usize,
    pub end: usize,
}

/// Builds a matcher: case-insensitive unless the query has uppercase letters.
pub fn compile(query: &str) -> Option<Regex> {
    if query.is_empty() {
        return None;
    }
    let insensitive = !query.chars().any(char::is_uppercase);
    RegexBuilder::new(query)
        .case_insensitive(insensitive)
        .build()
        .or_else(|_| {
            RegexBuilder::new(&regex::escape(query))
                .case_insensitive(insensitive)
                .build()
        })
        .ok()
}

pub fn find_all(lines: &[Line], re: &Regex) -> Vec<Match> {
    let mut out = Vec::new();
    for (n, line) in lines.iter().enumerate() {
        let text = line.text();
        out.extend(
            re.find_iter(&text)
                .filter(|m| !m.is_empty())
                .map(|m| Match {
                    line: n,
                    start: m.start(),
                    end: m.end(),
                }),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::Span;
    use ratatui::style::Style;

    fn line(t: &str) -> Line {
        Line {
            spans: vec![Span::new(t, Style::new())],
            ..Default::default()
        }
    }

    #[test]
    fn smart_case() {
        let lines = [line("Foo foo FOO")];
        assert_eq!(find_all(&lines, &compile("foo").unwrap()).len(), 3);
        assert_eq!(find_all(&lines, &compile("Foo").unwrap()).len(), 1);
    }

    #[test]
    fn invalid_regex_falls_back_to_literal() {
        let lines = [line("a (b c")];
        let m = find_all(&lines, &compile("(b").unwrap());
        assert_eq!(m, [Match { line: 0, start: 2, end: 4 }]);
    }
}
