//! Shared inline markup scanner for the lightweight formats (Org, AsciiDoc, RST).

use super::ast::Inline;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Strong,
    Emph,
    Code,
    Strike,
}

/// Per-format inline rules.
pub struct Syntax {
    /// Delimiter pairs (open == close), longest first.
    pub delims: &'static [(&'static str, Kind)],
    /// Format-specific constructs (links, images) tried at each position.
    /// Returns the inline and the number of bytes consumed.
    pub special: fn(&str) -> Option<(Inline, usize)>,
}

fn is_boundary(c: Option<char>) -> bool {
    c.is_none_or(|c| c.is_whitespace() || (c.is_ascii_punctuation() && c != '\\'))
}

pub fn parse(text: &str, syn: &Syntax) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut buf = String::new();
    let mut i = 0;
    let flush = |buf: &mut String, out: &mut Vec<Inline>| {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf)));
        }
    };
    'outer: while i < text.len() {
        let rest = &text[i..];
        let prev = text[..i].chars().next_back();

        if let Some((inl, used)) = (syn.special)(rest).or_else(|| autolink(rest, prev)) {
            flush(&mut buf, &mut out);
            out.push(inl);
            i += used;
            continue;
        }

        if is_boundary(prev) {
            for &(d, kind) in syn.delims {
                if let Some(end) = closing(rest, d) {
                    let inner = &rest[d.len()..end];
                    flush(&mut buf, &mut out);
                    out.push(match kind {
                        Kind::Code => Inline::Code(inner.to_string()),
                        Kind::Strong => Inline::Strong(parse(inner, syn)),
                        Kind::Emph => Inline::Emph(parse(inner, syn)),
                        Kind::Strike => Inline::Strike(parse(inner, syn)),
                    });
                    i += end + d.len();
                    continue 'outer;
                }
            }
        }

        let c = rest.chars().next().expect("non-empty");
        buf.push(c);
        i += c.len_utf8();
    }
    flush(&mut buf, &mut out);
    out
}

/// Byte offset of the closing delimiter if `rest` starts a valid span.
fn closing(rest: &str, d: &str) -> Option<usize> {
    let body = rest.strip_prefix(d)?;
    if body.starts_with(char::is_whitespace) || body.starts_with(d) {
        return None;
    }
    let mut from = 0;
    while let Some(pos) = body[from..].find(d) {
        let at = from + pos;
        let before = body[..at].chars().next_back();
        let after = body[at + d.len()..].chars().next();
        if at > 0 && before.is_some_and(|c| !c.is_whitespace()) && is_boundary(after) {
            return Some(d.len() + at);
        }
        from = at + d.len();
    }
    None
}

/// Bare `http(s)://` URLs become links.
fn autolink(rest: &str, prev: Option<char>) -> Option<(Inline, usize)> {
    if !(rest.starts_with("http://") || rest.starts_with("https://")) {
        return None;
    }
    if prev.is_some_and(|c| c.is_alphanumeric()) {
        return None;
    }
    let mut end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '[' | ']'))
        .unwrap_or(rest.len());
    while rest[..end].ends_with(['.', ',', ';', ':', ')', '!', '?']) {
        end -= 1;
    }
    let url = &rest[..end];
    Some((
        Inline::Link {
            url: url.to_string(),
            content: vec![Inline::Text(url.to_string())],
        },
        end,
    ))
}

/// Joins paragraph lines with soft breaks and parses inline markup.
pub fn paragraph(lines: &[&str], syn: &Syntax) -> Vec<Inline> {
    parse(
        &lines.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" "),
        syn,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYN: Syntax = Syntax {
        delims: &[("**", Kind::Strong), ("*", Kind::Emph), ("`", Kind::Code)],
        special: |_| None,
    };

    #[test]
    fn delimiters() {
        assert_eq!(
            parse("a **b** *c* `d*e`", &SYN),
            vec![
                Inline::Text("a ".into()),
                Inline::Strong(vec![Inline::Text("b".into())]),
                Inline::Text(" ".into()),
                Inline::Emph(vec![Inline::Text("c".into())]),
                Inline::Text(" ".into()),
                Inline::Code("d*e".into()),
            ]
        );
    }

    #[test]
    fn no_intraword_or_unclosed() {
        assert_eq!(
            parse("2*3*4 and *x", &SYN),
            vec![Inline::Text("2*3*4 and *x".into())]
        );
    }

    #[test]
    fn autolinks() {
        let v = parse("see https://x.dev/a.", &SYN);
        assert!(matches!(&v[1], Inline::Link { url, .. } if url == "https://x.dev/a"));
        assert_eq!(v[2], Inline::Text(".".into()));
    }
}
