//! Syntax highlighting for code blocks.

use crate::layout::Span;
use crate::theme::Theme;

/// Returns one span list per source line.
pub fn highlight(code: &str, _lang: Option<&str>, theme: &Theme) -> Vec<Vec<Span>> {
    code.lines()
        .map(|l| vec![Span::new(l, theme.code_text)])
        .collect()
}
