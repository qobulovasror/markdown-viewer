mod asciidoc;
pub mod ast;
#[cfg(test)]
mod fuzz_tests;
mod html;
mod inline;
mod lines;
pub mod markdown;
mod org;
mod rst;
mod text;

use std::path::Path;

pub use ast::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Org,
    AsciiDoc,
    Rst,
    Text,
}

impl Format {
    pub fn from_path(path: &Path) -> Format {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        match ext.as_str() {
            "org" => Format::Org,
            "adoc" | "asciidoc" | "asc" => Format::AsciiDoc,
            "rst" | "rest" => Format::Rst,
            "txt" | "text" | "log" => Format::Text,
            _ => Format::Markdown,
        }
    }

    pub fn from_name(name: &str) -> Option<Format> {
        Some(match name {
            "md" | "markdown" => Format::Markdown,
            "org" => Format::Org,
            "adoc" | "asciidoc" => Format::AsciiDoc,
            "rst" => Format::Rst,
            "txt" | "text" => Format::Text,
            _ => return None,
        })
    }
}

pub const FORMAT_NAMES: &[&str] = &["md", "org", "adoc", "rst", "txt"];

pub fn parse(src: &str, format: Format) -> Document {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut doc = match format {
        Format::Markdown => markdown::parse(src),
        Format::Org => org::parse(src),
        Format::AsciiDoc => asciidoc::parse(src),
        Format::Rst => rst::parse(src),
        Format::Text => text::parse(src),
    };
    for b in &mut doc.blocks {
        postprocess_block(b);
    }
    doc.blocks
        .retain(|b| !matches!(b, Block::Plain(v) if v.is_empty()));
    doc
}

fn postprocess_block(b: &mut Block) {
    match b {
        Block::Html(h) => {
            if let Some(heading) = html_heading(h) {
                *b = heading;
                return;
            }
            let inl = html::to_inlines(h);
            *b = if inl.is_empty() {
                Block::Plain(Vec::new())
            } else {
                Block::Paragraph(inl)
            };
            if let Block::Paragraph(inl) = b {
                postprocess_inlines(inl);
            }
        }
        Block::Heading { content: inl, .. } | Block::Paragraph(inl) | Block::Plain(inl) => {
            postprocess_inlines(inl)
        }
        Block::BlockQuote { blocks, .. } | Block::FootnoteDef { blocks, .. } => {
            blocks.iter_mut().for_each(postprocess_block)
        }
        Block::List { items, .. } => items
            .iter_mut()
            .flat_map(|i| i.blocks.iter_mut())
            .for_each(postprocess_block),
        Block::Table { header, rows, .. } => header
            .iter_mut()
            .chain(rows.iter_mut().flatten())
            .for_each(postprocess_inlines),
        Block::DefinitionList(entries) => {
            for (term, defs) in entries {
                postprocess_inlines(term);
                defs.iter_mut().flatten().for_each(postprocess_block);
            }
        }
        Block::CodeBlock { .. } | Block::Math(_) | Block::Rule => {}
    }
}

/// `<h1 ...>Title</h1>` blocks become real headings (shown in the TOC).
fn html_heading(html: &str) -> Option<Block> {
    let t = html.trim();
    let rest = t.strip_prefix("<h").or_else(|| t.strip_prefix("<H"))?;
    let level = rest
        .chars()
        .next()?
        .to_digit(10)
        .filter(|l| (1..=6).contains(l))? as u8;
    let close = format!("</h{level}>");
    if !t.to_ascii_lowercase().ends_with(&close) {
        return None;
    }
    let mut content = html::to_inlines(t);
    postprocess_inlines(&mut content);
    let id = markdown::slugify(&plain_text(&content));
    Some(Block::Heading { level, id, content })
}

/// Emoji shortcodes and inline HTML tags (`<a>`, `<sup>`, `<kbd>`, ...).
fn postprocess_inlines(inl: &mut Vec<Inline>) {
    let items = std::mem::take(inl);
    // Stack of (closing tag name, constructor, collected children).
    let mut stack: Vec<(String, Option<String>, Vec<Inline>)> = Vec::new();
    let mut out: Vec<Inline> = Vec::new();
    for mut i in items {
        match &mut i {
            Inline::Text(t) => *t = emoji(t),
            Inline::Emph(c)
            | Inline::Strong(c)
            | Inline::Strike(c)
            | Inline::Sup(c)
            | Inline::Sub(c) => postprocess_inlines(c),
            Inline::Link { content, .. } => postprocess_inlines(content),
            _ => {}
        }
        if let Inline::Html(h) = &i {
            let tag = h.trim();
            let closing = tag.starts_with("</");
            let name: String = tag
                .trim_start_matches(['<', '/'])
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect::<String>()
                .to_ascii_lowercase();
            const WRAPPERS: &[&str] = &[
                "a", "sup", "sub", "b", "strong", "i", "em", "kbd", "code", "del", "s", "u",
                "mark", "summary",
            ];
            if WRAPPERS.contains(&name.as_str()) {
                if !closing {
                    let href = (name == "a").then(|| {
                        match html::to_inlines(&format!("{tag}x</a>")).pop() {
                            Some(Inline::Link { url, .. }) => url,
                            _ => String::new(),
                        }
                    });
                    stack.push((name, href, Vec::new()));
                    continue;
                }
                if let Some(pos) = stack.iter().rposition(|(n, _, _)| *n == name) {
                    while stack.len() > pos + 1 {
                        let (_, _, children) = stack.pop().expect("non-empty");
                        target(&mut stack, &mut out).extend(children);
                    }
                    let (name, href, children) = stack.pop().expect("non-empty");
                    let wrapped = match name.as_str() {
                        "a" => match href {
                            Some(url) if !url.is_empty() => Inline::Link {
                                url,
                                content: children,
                            },
                            _ => Inline::Emph(children),
                        },
                        "sup" => Inline::Sup(children),
                        "sub" => Inline::Sub(children),
                        "b" | "strong" => Inline::Strong(children),
                        "kbd" | "code" => Inline::Code(plain_text(&children)),
                        "del" | "s" => Inline::Strike(children),
                        "summary" => {
                            let mut c = vec![Inline::Text("▸ ".into())];
                            c.extend(children);
                            Inline::Strong(c)
                        }
                        _ => Inline::Emph(children),
                    };
                    target(&mut stack, &mut out).push(wrapped);
                    continue;
                }
                continue;
            }
            if name == "img" {
                let imgs = html::to_inlines(tag);
                target(&mut stack, &mut out).extend(imgs);
                continue;
            }
        }
        target(&mut stack, &mut out).push(i);
    }
    while let Some((_, _, children)) = stack.pop() {
        target(&mut stack, &mut out).extend(children);
    }
    *inl = out;
}

fn target<'a>(
    stack: &'a mut [(String, Option<String>, Vec<Inline>)],
    out: &'a mut Vec<Inline>,
) -> &'a mut Vec<Inline> {
    match stack.last_mut() {
        Some((_, _, c)) => c,
        None => out,
    }
}

/// Replaces `:shortcode:` with the emoji, leaving unknown codes untouched.
fn emoji(text: &str) -> String {
    if !text.contains(':') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-'))
            .unwrap_or(after.len());
        if end > 0
            && after[end..].starts_with(':')
            && let Some(e) = emojis::get_by_shortcode(&after[..end])
        {
            out.push_str(e.as_str());
            rest = &after[end + 1..];
        } else {
            out.push(':');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_shortcodes() {
        assert_eq!(emoji("ship it :rocket: now"), "ship it 🚀 now");
        assert_eq!(
            emoji("time 10:30:00 and :nope:"),
            "time 10:30:00 and :nope:"
        );
    }

    #[test]
    fn inline_html_tags() {
        let doc = parse(
            "Press <kbd>Ctrl</kbd> or visit <a href=\"https://x.dev\">site</a>, x<sup>2</sup>.",
            Format::Markdown,
        );
        let Block::Paragraph(p) = &doc.blocks[0] else {
            panic!()
        };
        assert!(p.contains(&Inline::Code("Ctrl".into())));
        assert!(
            p.iter()
                .any(|i| matches!(i, Inline::Link { url, .. } if url == "https://x.dev"))
        );
        assert!(p.iter().any(|i| matches!(i, Inline::Sup(_))));
    }

    #[test]
    fn html_heading_and_summary() {
        let doc = parse(
            "<h1 align=\"center\">mdv</h1>\n\n<details>\n<summary><b>Keys</b></summary>\n\ntext\n\n</details>\n",
            Format::Markdown,
        );
        assert!(matches!(&doc.blocks[0], Block::Heading { level: 1, id, .. } if id == "mdv"));
        let Block::Paragraph(p) = &doc.blocks[1] else {
            panic!("{:?}", doc.blocks)
        };
        assert!(matches!(&p[0], Inline::Strong(_)));
        assert_eq!(plain_text(p), "▸ Keys");
        assert_eq!(doc.blocks.len(), 3);
    }

    #[test]
    fn html_block_image() {
        let doc = parse(
            "<p align=\"center\"><img src=\"a.png\" alt=\"Logo\"></p>\n",
            Format::Markdown,
        );
        assert_eq!(
            doc.blocks,
            [Block::Paragraph(vec![Inline::Image {
                url: "a.png".into(),
                alt: "Logo".into()
            }])]
        );
    }
}
