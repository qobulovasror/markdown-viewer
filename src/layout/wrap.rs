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

/// Borrowed slice of a span's text with its styling.
#[derive(Clone, Copy)]
struct Piece<'a> {
    text: &'a str,
    style: ratatui::style::Style,
    link: Option<usize>,
}

enum Tok<'a> {
    /// Range into the piece list, plus display width.
    Word(usize, usize, usize),
    Space(Piece<'a>),
    Break,
}

pub fn width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.text.width()).sum()
}

fn tokenize(segs: &[Seg]) -> (Vec<Piece<'_>>, Vec<Tok<'_>>) {
    let mut pieces = Vec::new();
    let mut toks = Vec::new();
    let mut word_start = 0;
    let mut word_w = 0;
    let finish = |pieces: &Vec<Piece>, toks: &mut Vec<Tok>, start: &mut usize, w: &mut usize| {
        if pieces.len() > *start {
            toks.push(Tok::Word(*start, pieces.len(), *w));
        }
        *start = pieces.len();
        *w = 0;
    };
    for seg in segs {
        match seg {
            Seg::Break => {
                finish(&pieces, &mut toks, &mut word_start, &mut word_w);
                toks.push(Tok::Break);
            }
            Seg::Space(s) => {
                finish(&pieces, &mut toks, &mut word_start, &mut word_w);
                toks.push(Tok::Space(Piece {
                    text: " ",
                    style: s.style,
                    link: s.link,
                }));
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
                        finish(&pieces, &mut toks, &mut word_start, &mut word_w);
                        toks.push(Tok::Space(Piece {
                            text: " ",
                            style: s.style,
                            link: s.link,
                        }));
                    } else {
                        word_w += run.width();
                        pieces.push(Piece {
                            text: run,
                            style: s.style,
                            link: s.link,
                        });
                    }
                    rest = tail;
                }
            }
        }
    }
    finish(&pieces, &mut toks, &mut word_start, &mut word_w);
    (pieces, toks)
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

fn push_piece(line: &mut Vec<Span>, p: Piece) {
    if p.text.is_empty() {
        return;
    }
    if let Some(last) = line.last_mut()
        && last.style == p.style
        && last.link == p.link
    {
        last.text.push_str(p.text);
        return;
    }
    line.push(Span {
        text: p.text.to_string(),
        style: p.style,
        link: p.link,
    });
}

/// Wraps segments into lines no wider than `max` columns.
pub fn wrap(segs: Vec<Seg>, max: usize) -> Vec<Vec<Span>> {
    let max = max.max(1);
    let mut lines = Vec::new();
    let mut cur: Vec<Span> = Vec::new();
    let mut cur_w = 0;
    let mut space: Option<Piece> = None;
    let (pieces, toks) = tokenize(&segs);

    for tok in toks {
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
            Tok::Word(a, b, w) => {
                let sp = usize::from(space.is_some());
                if cur_w > 0 && cur_w + sp + w > max {
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0;
                    space = None;
                } else if let Some(s) = space.take() {
                    push_piece(&mut cur, s);
                    cur_w += 1;
                }
                if w <= max - cur_w {
                    for &p in &pieces[a..b] {
                        push_piece(&mut cur, p);
                    }
                    cur_w += w;
                } else {
                    // Word longer than a line: break between graphemes.
                    for &p in &pieces[a..b] {
                        for g in p.text.graphemes(true) {
                            let gw = g.width();
                            if cur_w + gw > max && cur_w > 0 {
                                lines.push(std::mem::take(&mut cur));
                                cur_w = 0;
                            }
                            push_piece(&mut cur, Piece { text: g, ..p });
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

/// Width of the widest line if `segs` were never wrapped (whitespace runs count as one).
pub fn natural_width(segs: &[Seg]) -> usize {
    let (mut max, mut cur, mut pending_space) = (0, 0, false);
    let word = |w: usize, cur: &mut usize, pending: &mut bool| {
        if *pending && *cur > 0 {
            *cur += 1;
        }
        *pending = false;
        *cur += w;
    };
    for seg in segs {
        match seg {
            Seg::Break => {
                max = max.max(cur);
                cur = 0;
                pending_space = false;
            }
            Seg::Space(_) => pending_space = true,
            Seg::Span(s) => {
                for (i, part) in s.text.split(char::is_whitespace).enumerate() {
                    if i > 0 {
                        pending_space = true;
                    }
                    if !part.is_empty() {
                        word(part.width(), &mut cur, &mut pending_space);
                    }
                }
            }
        }
    }
    max.max(cur)
}

/// Hard-wraps a single line of spans at `max` columns (used for code).
pub fn hard_wrap(spans: &[Span], max: usize) -> Vec<Vec<Span>> {
    hard_wrap_hanging(spans, max, max)
}

/// Like `hard_wrap`, but continuation lines are limited to `rest` columns.
pub fn hard_wrap_hanging(spans: &[Span], first: usize, rest: usize) -> Vec<Vec<Span>> {
    let rest = rest.max(1);
    let mut max = first.max(1);
    let mut lines = Vec::new();
    let mut cur = Vec::new();
    let mut cur_w = 0;
    for s in spans {
        // Fast path: the whole span fits on the current line.
        let w = s.text.width();
        if cur_w + w <= max {
            cur_w += w;
            push_span(&mut cur, s.clone());
            continue;
        }
        let mut piece = String::new();
        for g in s.text.graphemes(true) {
            let gw = g.width();
            if cur_w + gw > max && cur_w > 0 {
                push_span(&mut cur, s.with_text(&std::mem::take(&mut piece)));
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
                max = rest;
            }
            piece.push_str(g);
            cur_w += gw;
        }
        push_span(&mut cur, s.with_text(&piece));
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
            vec![
                seg("aaa "),
                Seg::Span(Span::new("bb", Style::new().bold())),
                seg("cc"),
            ],
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
    fn hard_wrap_splits_spans() {
        let spans = vec![
            Span::new("abc", Style::new()),
            Span::new("defgh", Style::new().bold()),
        ];
        assert_eq!(text(&hard_wrap(&spans, 4)), ["abcd", "efgh"]);
    }

    #[test]
    fn hanging_wrap_uses_narrower_continuation() {
        let spans = vec![Span::new("abcdefghij", Style::new())];
        assert_eq!(
            text(&hard_wrap_hanging(&spans, 4, 2)),
            ["abcd", "ef", "gh", "ij"]
        );
    }

    #[test]
    fn natural_width_matches_unwrapped() {
        let segs = vec![
            seg("a  bb "),
            Seg::Span(Span::new("ccc", Style::new())),
            Seg::Break,
            seg("dddddd"),
        ];
        assert_eq!(natural_width(&segs), 8);
    }

    #[test]
    fn wide_chars() {
        let out = wrap(vec![seg("日本語テキスト")], 6);
        assert_eq!(text(&out), ["日本語", "テキス", "ト"]);
    }
}
