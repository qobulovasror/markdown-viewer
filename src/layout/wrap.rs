//! Greedy word wrapping of styled spans.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use super::Span;

/// Flattened inline content ready for wrapping.
#[derive(Debug, Clone)]
pub enum Seg {
    Span(Span),
    /// Soft break / whitespace between words.
    Space(Span),
    /// Forced line break.
    Break,
}

enum Tok {
    Word(Vec<Span>),
    Space(Span),
    Break,
}

pub fn width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.width()).sum()
}

fn tokenize(segs: Vec<Seg>) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut word: Vec<Span> = Vec::new();
    let finish = |word: &mut Vec<Span>, toks: &mut Vec<Tok>| {
        if !word.is_empty() {
            toks.push(Tok::Word(std::mem::take(word)));
        }
    };
    for seg in segs {
        match seg {
            Seg::Break => {
                finish(&mut word, &mut toks);
                toks.push(Tok::Break);
            }
            Seg::Space(s) => {
                finish(&mut word, &mut toks);
                toks.push(Tok::Space(s.with_text(" ")));
            }
            Seg::Span(s) => {
                let mut rest = s.text.as_str();
                while !rest.is_empty() {
                    let ws = rest.starts_with(char::is_whitespace);
                    let end = rest
                        .find(|c: char| c.is_whitespace() != ws)
                        .unwrap_or(rest.len());
                    let (run, tail) = rest.split_at(end);
                    if ws {
                        finish(&mut word, &mut toks);
                        toks.push(Tok::Space(s.with_text(" ")));
                    } else {
                        word.push(s.with_text(run));
                    }
                    rest = tail;
                }
            }
        }
    }
    finish(&mut word, &mut toks);
    toks
}

pub fn push_span(line: &mut Vec<Span>, s: Span) {
    if s.text.is_empty() {
        return;
    }
    if let Some(last) = line.last_mut()
        && last.style == s.style
        && last.link == s.link
    {
        last.text.push_str(&s.text);
        return;
    }
    line.push(s);
}

/// Wraps segments into lines no wider than `max` columns.
pub fn wrap(segs: Vec<Seg>, max: usize) -> Vec<Vec<Span>> {
    let max = max.max(1);
    let mut lines = Vec::new();
    let mut cur: Vec<Span> = Vec::new();
    let mut cur_w = 0;
    let mut space: Option<Span> = None;

    for tok in tokenize(segs) {
        match tok {
            Tok::Space(s) => {
                if cur_w > 0 && space.is_none() {
                    space = Some(s);
                }
            }
            Tok::Break => {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
                space = None;
            }
            Tok::Word(spans) => {
                let w = width(&spans);
                let sp = usize::from(space.is_some());
                if cur_w > 0 && cur_w + sp + w > max {
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0;
                    space = None;
                } else if let Some(s) = space.take() {
                    push_span(&mut cur, s);
                    cur_w += 1;
                }
                if w <= max - cur_w {
                    for s in spans {
                        push_span(&mut cur, s);
                    }
                    cur_w += w;
                } else {
                    // Word longer than a line: break between graphemes.
                    for s in spans {
                        for g in s.text.graphemes(true) {
                            let gw = g.width();
                            if cur_w + gw > max && cur_w > 0 {
                                lines.push(std::mem::take(&mut cur));
                                cur_w = 0;
                            }
                            push_span(&mut cur, s.with_text(g));
                            cur_w += gw;
                        }
                    }
                }
            }
        }
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Hard-wraps a single line of spans at `max` columns (used for code).
pub fn hard_wrap(spans: Vec<Span>, max: usize) -> Vec<Vec<Span>> {
    let max = max.max(1);
    let mut lines = Vec::new();
    let mut cur = Vec::new();
    let mut cur_w = 0;
    for s in spans {
        for g in s.text.graphemes(true) {
            let gw = g.width();
            if cur_w + gw > max && cur_w > 0 {
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
            }
            push_span(&mut cur, s.with_text(g));
            cur_w += gw;
        }
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Style;

    fn text(lines: &[Vec<Span>]) -> Vec<String> {
        lines
            .iter()
            .map(|l| l.iter().map(|s| s.text.as_str()).collect())
            .collect()
    }

    fn seg(t: &str) -> Seg {
        Seg::Span(Span::new(t, Style::new()))
    }

    #[test]
    fn wraps_on_words() {
        let out = wrap(vec![seg("the quick brown fox jumps")], 10);
        assert_eq!(text(&out), ["the quick", "brown fox", "jumps"]);
    }

    #[test]
    fn breaks_long_words() {
        let out = wrap(vec![seg("abcdefghij kl")], 4);
        assert_eq!(text(&out), ["abcd", "efgh", "ij", "kl"]);
    }

    #[test]
    fn word_spanning_styles_stays_together() {
        let out = wrap(
            vec![seg("aaa "), Seg::Span(Span::new("bb", Style::new().bold())), seg("cc")],
            5,
        );
        assert_eq!(text(&out), ["aaa", "bbcc"]);
    }

    #[test]
    fn hard_break() {
        let out = wrap(vec![seg("a"), Seg::Break, seg("b")], 10);
        assert_eq!(text(&out), ["a", "b"]);
    }

    #[test]
    fn wide_chars() {
        let out = wrap(vec![seg("日本語テキスト")], 6);
        assert_eq!(text(&out), ["日本語", "テキス", "ト"]);
    }
}
