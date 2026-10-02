//! Minimal HTML fragment handling: keeps text, images, links and line breaks.

use super::ast::Inline;

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
