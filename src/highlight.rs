//! Syntax highlighting for code blocks (syntect + bat's extra syntaxes).

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use ratatui::style::{Color, Modifier, Style};
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

use crate::layout::Span;
use crate::theme::Theme;

pub type Lines = Arc<Vec<Vec<Span>>>;

fn syntaxes() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(two_face::syntax::extra_newlines)
}

fn themes() -> &'static ThemeSet {
    static SET: OnceLock<ThemeSet> = OnceLock::new();
    SET.get_or_init(ThemeSet::load_defaults)
}

/// Results keyed by (code, lang, theme) so relayout on resize stays cheap.
fn cache() -> &'static Mutex<HashMap<u64, Lines>> {
    static CACHE: OnceLock<Mutex<HashMap<u64, Lines>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn find_syntax<'a>(
    set: &'a SyntaxSet,
    lang: Option<&str>,
    code: &str,
) -> Option<&'a SyntaxReference> {
    if let Some(lang) = lang {
        let lang = lang.to_ascii_lowercase();
        let token = match lang.as_str() {
            "shell" | "zsh" | "console" | "shellscript" => "bash",
            "py" | "python3" => "python",
            "rs" => "rust",
            "ts" => "typescript",
            "yml" => "yaml",
            "dockerfile" => "Dockerfile",
            "text" | "txt" | "plain" | "plaintext" => return None,
            other => other,
        };
        if let Some(s) = set
            .find_syntax_by_token(token)
            .or_else(|| set.find_syntax_by_name(token))
        {
            return Some(s);
        }
    }
    // No (known) language: try the first line, e.g. a shebang.
    set.find_syntax_by_first_line(code.lines().next()?)
}

/// Returns one span list per source line.
pub fn highlight(code: &str, lang: Option<&str>, theme: &Theme) -> Lines {
    let plain = || {
        Arc::new(
            code.lines()
                .map(|l| vec![Span::new(l, theme.code_text)])
                .collect(),
        )
    };
    let set = syntaxes();
    let Some(syntax) = find_syntax(set, lang, code) else {
        return plain();
    };
    let Some(st) = themes().themes.get(theme.syntax) else {
        return plain();
    };

    let mut h = DefaultHasher::new();
    (code, lang, theme.syntax).hash(&mut h);
    let key = h.finish();
    if let Some(hit) = cache().lock().ok().and_then(|c| c.get(&key).cloned()) {
        return hit;
    }

    let mut hl = HighlightLines::new(syntax, st);
    let mut out = Vec::new();
    for line in code.split_inclusive('\n') {
        let Ok(ranges) = hl.highlight_line(line, set) else {
            return plain();
        };
        let spans = ranges
            .into_iter()
            .filter_map(|(s, text)| {
                let text = text.trim_end_matches(['\n', '\r']);
                (!text.is_empty()).then(|| Span::new(text, convert(s)))
            })
            .collect();
        out.push(spans);
    }
    let out = Arc::new(out);
    if let Ok(mut c) = cache().lock() {
        c.insert(key, out.clone());
    }
    out
}

fn convert(s: syntect::highlighting::Style) -> Style {
    let f = s.foreground;
    let mut style = Style::new().fg(Color::Rgb(f.r, f.g, f.b));
    if s.font_style.contains(FontStyle::BOLD) {
        style = style.add_modifier(Modifier::BOLD);
    }
    if s.font_style.contains(FontStyle::ITALIC) {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if s.font_style.contains(FontStyle::UNDERLINE) {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    style
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_known_language() {
        let t = Theme::default_dark();
        let lines = highlight("fn main() {}\nlet x = 1;", Some("rust"), &t);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].len() > 1, "expected several styled tokens");
    }

    #[test]
    fn detects_shebang() {
        let set = syntaxes();
        let s = find_syntax(set, None, "#!/bin/bash\necho hi").unwrap();
        assert!(
            s.name.contains("Bash") || s.name.contains("Shell"),
            "{}",
            s.name
        );
    }

    #[test]
    fn unknown_language_is_plain() {
        let t = Theme::default_dark();
        let lines = highlight("abc", Some("nosuchlang"), &t);
        assert_eq!(*lines, vec![vec![Span::new("abc", t.code_text)]]);
    }

    #[test]
    fn all_theme_syntaxes_exist() {
        for name in crate::theme::THEME_NAMES {
            let t = Theme::by_name(name).unwrap();
            assert!(themes().themes.contains_key(t.syntax), "{}", t.syntax);
        }
    }
}
