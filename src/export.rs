//! Standalone HTML export of a parsed document (`mdvw --export html`).
//!
//! Raw HTML from the document is never copied through, and links with
//! scriptable schemes are dropped, so the page is safe to open.

use std::fmt::Write;

use crate::parser::{Admonition, Align, Block, Document, Inline, ListItem, plain_text};
use crate::theme::Theme;

/// A complete HTML page for `doc`.
pub fn to_html(doc: &Document, title: &str, theme: &Theme) -> String {
    let mut body = String::new();
    blocks(&doc.blocks, theme, &mut body);
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <meta name=\"generator\" content=\"mdvw {}\">\n<title>{}</title>\n<style>{CSS}</style>\n\
         </head>\n<body>\n<main>\n{body}</main>\n</body>\n</html>\n",
        env!("CARGO_PKG_VERSION"),
        esc(title),
    )
}

/// Title for the page: the first heading, else `fallback`.
pub fn title(doc: &Document, fallback: &str) -> String {
    doc.blocks
        .iter()
        .find_map(|b| match b {
            Block::Heading { content, .. } => Some(plain_text(content)),
            _ => None,
        })
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

const CSS: &str = "
:root{color-scheme:light dark;--fg:#1f2328;--bg:#fff;--muted:#59636e;--line:#d1d9e0;--code:#f6f8fa;--link:#0969da}
@media (prefers-color-scheme:dark){:root{--fg:#e6edf3;--bg:#0d1117;--muted:#9198a1;--line:#3d444d;--code:#151b23;--link:#4493f8}}
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.6 -apple-system,BlinkMacSystemFont,'Segoe UI',Helvetica,Arial,sans-serif}
main{max-width:860px;margin:0 auto;padding:32px 16px}
h1,h2{border-bottom:1px solid var(--line);padding-bottom:.3em}
a{color:var(--link)}
code{background:var(--code);padding:.15em .35em;border-radius:6px;font:85% ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
pre{background:var(--code);padding:16px;border-radius:6px;overflow:auto;font:85%/1.45 ui-monospace,SFMono-Regular,Menlo,Consolas,monospace}
pre code{background:none;padding:0;font-size:inherit}
blockquote{margin:0 0 16px;padding:0 1em;color:var(--muted);border-left:.25em solid var(--line)}
blockquote.admonition{color:var(--fg)}
blockquote.note{border-color:#0969da}blockquote.tip{border-color:#1a7f37}blockquote.important{border-color:#8250df}
blockquote.warning{border-color:#9a6700}blockquote.caution{border-color:#cf222e}
.admonition-title{font-weight:600}
table{border-collapse:collapse;display:block;overflow:auto}
th,td{border:1px solid var(--line);padding:6px 13px}
img{max-width:100%}
hr{border:0;border-top:1px solid var(--line)}
.math{font-style:italic}
.footnote{font-size:90%;color:var(--muted)}
ul.tasks{list-style:none;padding-left:1.2em}
";

fn blocks(bs: &[Block], theme: &Theme, out: &mut String) {
    for b in bs {
        block(b, theme, out);
    }
}

fn block(b: &Block, theme: &Theme, out: &mut String) {
    match b {
        Block::Heading { level, id, content } => {
            let l = (*level).clamp(1, 6);
            let _ = writeln!(out, "<h{l} id=\"{}\">{}</h{l}>", esc(id), inlines(content));
        }
        Block::Paragraph(inl) | Block::Plain(inl) => {
            let _ = writeln!(out, "<p>{}</p>", inlines(inl));
        }
        Block::CodeBlock { lang, code } => code_block(lang.as_deref(), code, theme, out),
        Block::BlockQuote {
            kind,
            blocks: inner,
        } => {
            match kind {
                Some(k) => {
                    let (class, label) = admonition(*k);
                    let _ = writeln!(
                        out,
                        "<blockquote class=\"admonition {class}\"><p class=\"admonition-title\">{label}</p>"
                    );
                }
                None => out.push_str("<blockquote>\n"),
            }
            blocks(inner, theme, out);
            out.push_str("</blockquote>\n");
        }
        Block::List {
            start,
            tight: _,
            items,
        } => list(*start, items, theme, out),
        Block::Table {
            aligns,
            header,
            rows,
        } => {
            out.push_str("<table>\n<thead><tr>");
            for (i, cell) in header.iter().enumerate() {
                let _ = write!(out, "<th{}>{}</th>", align(aligns, i), inlines(cell));
            }
            out.push_str("</tr></thead>\n<tbody>\n");
            for row in rows {
                out.push_str("<tr>");
                for (i, cell) in row.iter().enumerate() {
                    let _ = write!(out, "<td{}>{}</td>", align(aligns, i), inlines(cell));
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</tbody>\n</table>\n");
        }
        Block::DefinitionList(entries) => {
            out.push_str("<dl>\n");
            for (term, defs) in entries {
                let _ = writeln!(out, "<dt>{}</dt>", inlines(term));
                for d in defs {
                    out.push_str("<dd>");
                    blocks(d, theme, out);
                    out.push_str("</dd>\n");
                }
            }
            out.push_str("</dl>\n");
        }
        Block::FootnoteDef {
            label,
            blocks: inner,
        } => {
            let _ = write!(
                out,
                "<div class=\"footnote\" id=\"fn-{}\"><sup>{}</sup> ",
                esc(label),
                esc(label)
            );
            blocks(inner, theme, out);
            out.push_str("</div>\n");
        }
        Block::Math(m) => {
            let _ = writeln!(
                out,
                "<p class=\"math\">{}</p>",
                esc(&crate::math::to_unicode(m))
            );
        }
        // Raw HTML is not copied; its text was already kept by the parser.
        Block::Html(_) => {}
        Block::Rule => out.push_str("<hr>\n"),
    }
}

fn admonition(k: Admonition) -> (&'static str, &'static str) {
    match k {
        Admonition::Note => ("note", "Note"),
        Admonition::Tip => ("tip", "Tip"),
        Admonition::Important => ("important", "Important"),
        Admonition::Warning => ("warning", "Warning"),
        Admonition::Caution => ("caution", "Caution"),
    }
}

fn align(aligns: &[Align], i: usize) -> &'static str {
    match aligns.get(i) {
        Some(Align::Left) => " style=\"text-align:left\"",
        Some(Align::Center) => " style=\"text-align:center\"",
        Some(Align::Right) => " style=\"text-align:right\"",
        _ => "",
    }
}

fn list(start: Option<u64>, items: &[ListItem], theme: &Theme, out: &mut String) {
    let tasks = items.iter().any(|i| i.task.is_some());
    match start {
        Some(1) => out.push_str("<ol>\n"),
        Some(n) => {
            let _ = writeln!(out, "<ol start=\"{n}\">");
        }
        None if tasks => out.push_str("<ul class=\"tasks\">\n"),
        None => out.push_str("<ul>\n"),
    }
    for item in items {
        out.push_str("<li>");
        if let Some(done) = item.task {
            let checked = if done { " checked" } else { "" };
            let _ = write!(out, "<input type=\"checkbox\" disabled{checked}> ");
        }
        // Tight items: no paragraph around their text.
        for b in &item.blocks {
            match b {
                Block::Plain(inl) => out.push_str(&inlines(inl)),
                other => block(other, theme, out),
            }
        }
        out.push_str("</li>\n");
    }
    out.push_str(if start.is_some() {
        "</ol>\n"
    } else {
        "</ul>\n"
    });
}

fn code_block(lang: Option<&str>, code: &str, theme: &Theme, out: &mut String) {
    if lang.is_some_and(|l| l.eq_ignore_ascii_case("mermaid"))
        && let Some(lines) = crate::mermaid::render(code, 200)
    {
        let _ = writeln!(
            out,
            "<pre class=\"diagram\">{}</pre>",
            esc(&lines.join("\n"))
        );
        return;
    }
    match crate::highlight::to_html(code, lang, theme) {
        Some(html) => out.push_str(&html),
        None => {
            let _ = writeln!(out, "<pre><code>{}</code></pre>", esc(code));
        }
    }
}

fn inlines(inl: &[Inline]) -> String {
    let mut out = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => out.push_str(&esc(t)),
            Inline::Code(c) => {
                let _ = write!(out, "<code>{}</code>", esc(c));
            }
            Inline::Emph(c) => {
                let _ = write!(out, "<em>{}</em>", inlines(c));
            }
            Inline::Strong(c) => {
                let _ = write!(out, "<strong>{}</strong>", inlines(c));
            }
            Inline::Strike(c) => {
                let _ = write!(out, "<del>{}</del>", inlines(c));
            }
            Inline::Sup(c) => {
                let _ = write!(out, "<sup>{}</sup>", inlines(c));
            }
            Inline::Sub(c) => {
                let _ = write!(out, "<sub>{}</sub>", inlines(c));
            }
            Inline::Link { url, content } => {
                let _ = write!(
                    out,
                    "<a href=\"{}\">{}</a>",
                    esc(&safe_url(url)),
                    inlines(content)
                );
            }
            Inline::Image { url, alt } => {
                let _ = write!(
                    out,
                    "<img src=\"{}\" alt=\"{}\">",
                    esc(&safe_url(url)),
                    esc(alt)
                );
            }
            Inline::FootnoteRef(l) => {
                let _ = write!(out, "<sup><a href=\"#fn-{0}\">{0}</a></sup>", esc(l));
            }
            Inline::Math(m) | Inline::DisplayMath(m) => {
                let _ = write!(
                    out,
                    "<span class=\"math\">{}</span>",
                    esc(&crate::math::to_unicode(m))
                );
            }
            Inline::Html(h) => {
                // Only line breaks survive; other tags were turned into text.
                if h.trim().to_ascii_lowercase().starts_with("<br") {
                    out.push_str("<br>");
                }
            }
            Inline::SoftBreak => out.push('\n'),
            Inline::HardBreak => out.push_str("<br>\n"),
        }
    }
    out
}

/// Drops URLs whose scheme could run script when clicked.
fn safe_url(url: &str) -> String {
    let compact: String = url
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect::<String>()
        .to_ascii_lowercase();
    let blocked = ["javascript:", "vbscript:", "data:", "file:"];
    if blocked.iter().any(|b| compact.starts_with(b)) {
        "#".into()
    } else {
        url.to_string()
    }
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c if c.is_control() && c != '\n' && c != '\t' => out.push('\u{fffd}'),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::{Format, parse};

    fn export(src: &str) -> String {
        let doc = parse(src, Format::Markdown);
        to_html(&doc, &title(&doc, "x"), &Theme::default_dark())
    }

    #[test]
    fn exports_common_blocks() {
        let html = export(
            "# Hello & World\n\nSome *em* and `code`.\n\n- [x] done\n- [ ] todo\n\n| a | b |\n|:-|-:|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n\n> [!NOTE]\n> careful\n",
        );
        assert!(html.contains("<title>Hello &amp; World</title>"));
        assert!(
            html.contains("<h1 id=\"hello--world\">") || html.contains("<h1 id=\"hello-world\">")
        );
        assert!(html.contains("<em>em</em>") && html.contains("<code>code</code>"));
        assert!(html.contains("type=\"checkbox\" disabled checked"));
        assert!(html.contains("<td style=\"text-align:right\">2</td>"));
        assert!(html.contains("<pre style="), "syntect output");
        assert!(html.contains("class=\"admonition note\""));
    }

    #[test]
    fn untrusted_content_is_neutralised() {
        let html = export(
            "<script>alert(1)</script>\n\n[x](javascript:alert(1)) [y]( JaVaScRiPt:x) ![i](data:text/html,x)\n\n<img src=x onerror=alert(1)>\n",
        );
        assert!(!html.contains("<script"), "{html}");
        assert!(!html.to_ascii_lowercase().contains("javascript:"));
        assert!(!html.contains("data:text"));
        assert!(!html.contains("onerror"));
    }
}
