//! Minimal HTML fragment handling: keeps text, images, links and line breaks.

use super::ast::{Align, Block, Inline};

/// Converts an HTML fragment into inline content.
pub fn to_inlines(html: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut link: Option<(String, Vec<Inline>)> = None;
    let mut rest = html;
    let push =
        |i: Inline, link: &mut Option<(String, Vec<Inline>)>, out: &mut Vec<Inline>| match link {
            Some((_, content)) => content.push(i),
            None => out.push(i),
        };
    while !rest.is_empty() {
        let Some(start) = rest.find('<') else {
            push_text(rest, &mut link, &mut out);
            break;
        };
        push_text(&rest[..start], &mut link, &mut out);
        let after = &rest[start..];
        if after.starts_with("<!--") {
            rest = after.find("-->").map_or("", |e| &after[e + 3..]);
            continue;
        }
        let Some(end) = after.find('>') else {
            push_text(after, &mut link, &mut out);
            break;
        };
        let tag = &after[1..end];
        rest = &after[end + 1..];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let closing = tag.starts_with('/');
        match name.as_str() {
            "img" => {
                let url = attr(tag, "src").unwrap_or_default();
                let alt = attr(tag, "alt").unwrap_or_default();
                push(Inline::Image { url, alt }, &mut link, &mut out);
            }
            "br" => push(Inline::HardBreak, &mut link, &mut out),
            // Formatting tags are grouped later by `postprocess_inlines`.
            "b" | "strong" | "i" | "em" | "code" | "kbd" | "sup" | "sub" | "del" | "s" | "u"
            | "mark" | "summary" => push(Inline::Html(format!("<{tag}>")), &mut link, &mut out),
            "a" if !closing => {
                if let Some(href) = attr(tag, "href") {
                    link = Some((href, Vec::new()));
                }
            }
            "a" => {
                if let Some((url, content)) = link.take() {
                    out.push(Inline::Link { url, content });
                }
            }
            "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" if closing => {
                push(Inline::HardBreak, &mut link, &mut out)
            }
            _ => {}
        }
    }
    if let Some((url, content)) = link {
        out.push(Inline::Link { url, content });
    }
    // Collapse leading/trailing and repeated breaks.
    let mut cleaned: Vec<Inline> = Vec::new();
    for i in out {
        let is_break = matches!(i, Inline::HardBreak);
        if is_break && matches!(cleaned.last(), None | Some(Inline::HardBreak)) {
            continue;
        }
        cleaned.push(i);
    }
    while matches!(cleaned.last(), Some(Inline::HardBreak))
        || matches!(cleaned.last(), Some(Inline::Text(t)) if t.trim().is_empty())
    {
        cleaned.pop();
    }
    cleaned
}

fn push_text(text: &str, link: &mut Option<(String, Vec<Inline>)>, out: &mut Vec<Inline>) {
    let decoded = decode_entities(&text.split_whitespace().collect::<Vec<_>>().join(" "));
    if decoded.is_empty() {
        // Whitespace between elements (e.g. badge images) still separates them.
        let target = match link {
            Some((_, content)) => content,
            None => out,
        };
        if !text.is_empty() && !matches!(target.last(), None | Some(Inline::HardBreak)) {
            target.push(Inline::Text(" ".into()));
        }
        return;
    }
    // Keep a separating space where the source had whitespace.
    let lead = text.starts_with(char::is_whitespace);
    let trail = text.ends_with(char::is_whitespace);
    let s = format!(
        "{}{decoded}{}",
        if lead { " " } else { "" },
        if trail { " " } else { "" }
    );
    match link {
        Some((_, content)) => content.push(Inline::Text(s)),
        None => out.push(Inline::Text(s)),
    }
}

/// A simple HTML `<table>` (rows of `<th>`/`<td>`) as a table block.
/// The first row is the header; `align` / `text-align` of its cells set alignment.
pub fn to_table(html: &str) -> Option<Block> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<table")?;
    let end = lower[start..]
        .find("</table")
        .map_or(html.len(), |e| start + e);
    // Only a block that is just the table; text around it would be lost.
    let after = lower[end..].find('>').map_or(html.len(), |i| end + i + 1);
    let outside = format!("{}{}", &html[..start], &html[after..]);
    if !super::ast::plain_text(&to_inlines(&outside))
        .trim()
        .is_empty()
    {
        return None;
    }
    let mut rows: Vec<Vec<(String, String)>> = Vec::new();
    for row in split_tags(&lower, html, start, end, &["<tr"], &["</tr"]) {
        let (rs, re) = row;
        let cells = split_tags(&lower, html, rs, re, &["<td", "<th"], &["</td", "</th"])
            .into_iter()
            .map(|(cs, ce)| {
                // The opening tag (for attributes) and the cell's inner HTML.
                let open_end = html[cs..ce].find('>').map_or(ce, |i| cs + i + 1);
                (
                    html[cs..open_end].to_string(),
                    html[open_end..ce].to_string(),
                )
            })
            .collect::<Vec<_>>();
        if !cells.is_empty() {
            rows.push(cells);
        }
    }
    if rows.is_empty() {
        return None;
    }
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let aligns = (0..cols)
        .map(|i| {
            let tag = rows[0].get(i).map_or("", |(t, _)| t.as_str());
            let style = attr(tag, "style").unwrap_or_default().to_ascii_lowercase();
            let align = attr(tag, "align").unwrap_or_default().to_ascii_lowercase();
            let pick = |k: &str| {
                align == k
                    || style.contains(&format!("text-align:{k}"))
                    || style.contains(&format!("text-align: {k}"))
            };
            if pick("right") {
                Align::Right
            } else if pick("center") {
                Align::Center
            } else if pick("left") {
                Align::Left
            } else {
                Align::None
            }
        })
        .collect();
    let mut cells = rows.into_iter().map(|r| {
        let mut out: Vec<Vec<Inline>> = r.iter().map(|(_, inner)| to_inlines(inner)).collect();
        out.resize(cols, Vec::new());
        out
    });
    let header = cells.next()?;
    Some(Block::Table {
        aligns,
        header,
        rows: cells.collect(),
    })
}

/// Byte ranges from each opening tag (any of `opens`) to its close, the next
/// opening tag, or `end`, searching the lowercased copy.
fn split_tags(
    lower: &str,
    html: &str,
    from: usize,
    end: usize,
    opens: &[&str],
    closes: &[&str],
) -> Vec<(usize, usize)> {
    let find_any = |at: usize, pats: &[&str]| -> Option<usize> {
        pats.iter()
            .filter_map(|p| {
                let mut i = at;
                // `<th` must not match `<thead`.
                while let Some(k) = lower[i..end].find(p) {
                    let pos = i + k;
                    let next = lower.as_bytes().get(pos + p.len()).copied();
                    if matches!(next, Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/')) {
                        return Some(pos);
                    }
                    i = pos + p.len();
                }
                None
            })
            .min()
    };
    let mut out = Vec::new();
    let mut at = from;
    while at < end {
        let Some(s) = find_any(at, opens) else {
            break;
        };
        let after = s + 1;
        let stop = [find_any(after, closes), find_any(after, opens)]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or(end);
        out.push((s, stop.min(html.len())));
        at = stop.max(after);
    }
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = tag[at + name.len()..].trim_start();
        if !before_ok || !rest.starts_with('=') {
            continue;
        }
        let rest = rest[1..].trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or(""),
        };
        return Some(decode_entities(value));
    }
    None
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&copy;", "©")
        .replace("&mdash;", "—")
        .replace("&ndash;", "–")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_images_and_links() {
        let inl = to_inlines(
            r#"<p align="center"><img src="logo.png" alt="Logo" width=200></p><a href="https://x.dev">site</a>"#,
        );
        assert_eq!(
            inl,
            vec![
                Inline::Image {
                    url: "logo.png".into(),
                    alt: "Logo".into()
                },
                Inline::HardBreak,
                Inline::Link {
                    url: "https://x.dev".into(),
                    content: vec![Inline::Text("site".into())]
                },
            ]
        );
    }

    #[test]
    fn decodes_entities_and_skips_comments() {
        let inl = to_inlines("<!-- hidden -->Tom &amp; Jerry");
        assert_eq!(inl, vec![Inline::Text("Tom & Jerry".into())]);
    }
}
