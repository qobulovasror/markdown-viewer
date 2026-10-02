//! Org-mode parser (common subset).

use super::ast::*;
use super::inline::{self, Kind, Syntax};
use super::lines::*;

const SYNTAX: Syntax = Syntax {
    delims: &[
        ("*", Kind::Strong),
        ("/", Kind::Emph),
        ("_", Kind::Emph),
        ("=", Kind::Code),
        ("~", Kind::Code),
        ("+", Kind::Strike),
    ],
    special: link,
};

/// `[[url][description]]` or `[[url]]`.
fn link(rest: &str) -> Option<(Inline, usize)> {
    let body = rest.strip_prefix("[[")?;
    let end = body.find("]]")?;
    let inner = &body[..end];
    let (url, desc) = match inner.split_once("][") {
        Some((u, d)) => (u, Some(d)),
        None => (inner, None),
    };
    let url = url.strip_prefix("file:").unwrap_or(url);
    let used = 2 + end + 2;
    let is_image = desc.is_none()
        && [".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"]
            .iter()
            .any(|e| url.to_ascii_lowercase().ends_with(e));
    if is_image {
        return Some((
            Inline::Image {
                url: url.to_string(),
                alt: String::new(),
            },
            used,
        ));
    }
    let content = match desc {
        Some(d) => inline::parse(d, &SYNTAX),
        None => vec![Inline::Text(url.to_string())],
    };
    // Internal `*Heading` / `#custom-id` targets.
    let url = match url.strip_prefix('*') {
        Some(h) => format!("#{}", super::markdown::slugify(h)),
        None => url.to_string(),
    };
    Some((Inline::Link { url, content }, used))
}

pub fn parse(src: &str) -> Document {
    let lines: Vec<&str> = src.lines().collect();
    let mut front = Vec::new();
    let mut start = 0;
    while let Some(l) = lines.get(start) {
        let t = l.trim();
        if t.is_empty() {
            start += 1;
        } else if let Some((k, v)) = keyword(t)
            && !k.to_ascii_lowercase().starts_with("begin")
        {
            front.push(format!("{}: {v}", k.to_ascii_lowercase()));
            start += 1;
        } else {
            break;
        }
    }
    let mut p = OrgParser {
        slugs: Slugger::default(),
    };
    Document {
        front_matter: (!front.is_empty()).then(|| front.join("\n")),
        blocks: p.blocks(&lines[start..]),
    }
}

/// `#+KEY: value`
fn keyword(t: &str) -> Option<(&str, &str)> {
    let rest = t.strip_prefix("#+")?;
    let end = rest.find(|c: char| c == ':' || c.is_whitespace()).unwrap_or(rest.len());
    let (k, v) = rest.split_at(end);
    Some((k, v.strip_prefix(':').unwrap_or(v).trim()))
}

fn marker(line: &str) -> Option<Marker> {
    let m = simple_marker(line, &['-', '+', '*'], true)?;
    // A star at column 0 is a heading, not a bullet.
    (!(m.indent == 0 && line.starts_with('*'))).then_some(m)
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let stars = line.chars().take_while(|&c| c == '*').count();
    if stars == 0 || !line[stars..].starts_with(' ') {
        return None;
    }
    let mut text = line[stars..].trim();
    // Drop trailing :tags:
    if let Some(last) = text.split_whitespace().last()
        && last.len() > 2
        && last.starts_with(':')
        && last.ends_with(':')
    {
        text = text[..text.len() - last.len()].trim_end();
    }
    Some((stars.min(6) as u8, text))
}

struct OrgParser {
    slugs: Slugger,
}

impl OrgParser {
    fn blocks(&mut self, lines: &[&str]) -> Vec<Block> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            let t = line.trim();
            if t.is_empty() || (t.starts_with("# ") || t == "#") {
                i += 1;
                continue;
            }
            if let Some((level, text)) = heading(line) {
                out.push(self.slugs.heading(level, inline::parse(text, &SYNTAX)));
                i += 1;
                continue;
            }
            if let Some((kw, arg)) = keyword(t) {
                let kw = kw.to_ascii_lowercase();
                if let Some(kind) = kw.strip_prefix("begin_") {
                    let end_tag = format!("#+end_{kind}");
                    let end = lines[i + 1..]
                        .iter()
                        .position(|l| l.trim().to_ascii_lowercase().starts_with(&end_tag))
                        .map_or(lines.len(), |p| i + 1 + p);
                    let body = &lines[i + 1..end];
                    let base = body.iter().filter(|l| !is_blank(l)).map(|l| indent(l)).min().unwrap_or(0);
                    let body = dedent(body, base);
                    out.push(match kind {
                        "src" => Block::CodeBlock {
                            lang: arg.split_whitespace().next().map(str::to_string),
                            code: join(&body),
                        },
                        "example" | "verse" | "export" => Block::CodeBlock {
                            lang: None,
                            code: join(&body),
                        },
                        "quote" => Block::BlockQuote {
                            kind: None,
                            blocks: self.blocks(&body),
                        },
                        "note" | "tip" | "warning" | "important" | "caution" => Block::BlockQuote {
                            kind: Some(admonition(kind)),
                            blocks: self.blocks(&body),
                        },
                        _ => Block::BlockQuote {
                            kind: None,
                            blocks: self.blocks(&body),
                        },
                    });
                    i = end + 1;
                } else {
                    i += 1;
                }
                continue;
            }
            if t.starts_with('|') {
                let n = lines[i..].iter().take_while(|l| l.trim().starts_with('|')).count();
                out.push(table(&lines[i..i + n]));
                i += n;
                continue;
            }
            if t.len() >= 5 && t.chars().all(|c| c == '-') {
                out.push(Block::Rule);
                i += 1;
                continue;
            }
            if t.starts_with(": ") || t == ":" {
                let n = lines[i..]
                    .iter()
                    .take_while(|l| l.trim().starts_with(": ") || l.trim() == ":")
                    .count();
                let code: Vec<&str> = lines[i..i + n]
                    .iter()
                    .map(|l| l.trim().strip_prefix(':').unwrap_or("").strip_prefix(' ').unwrap_or(""))
                    .collect();
                out.push(Block::CodeBlock {
                    lang: None,
                    code: join(&code),
                });
                i += n;
                continue;
            }
            if marker(line).is_some() {
                let (list, used) = collect_list(&lines[i..], marker, |b| self.blocks(b));
                out.push(list);
                i += used;
                continue;
            }
            let n = lines[i..]
                .iter()
                .take_while(|l| {
                    !is_blank(l) && heading(l).is_none() && keyword(l.trim()).is_none() && marker(l).is_none() && !l.trim().starts_with('|')
                })
                .count()
                .max(1);
            out.push(Block::Paragraph(inline::paragraph(&lines[i..i + n], &SYNTAX)));
            i += n;
        }
        out
    }
}

fn admonition(kind: &str) -> Admonition {
    match kind {
        "tip" => Admonition::Tip,
        "warning" => Admonition::Warning,
        "important" => Admonition::Important,
        "caution" => Admonition::Caution,
        _ => Admonition::Note,
    }
}

fn join(lines: &[&str]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

fn table(lines: &[&str]) -> Block {
    let is_sep = |l: &str| l.trim().starts_with("|-");
    let rows: Vec<Vec<Vec<Inline>>> = lines
        .iter()
        .filter(|l| !is_sep(l))
        .map(|l| table_cells(l).into_iter().map(|c| inline::parse(c, &SYNTAX)).collect())
        .collect();
    let has_header = lines.len() > 1 && is_sep(lines[1]);
    let (header, rows) = match (has_header, rows.split_first()) {
        (true, Some((h, rest))) => (h.clone(), rest.to_vec()),
        _ => (Vec::new(), rows),
    };
    let ncols = rows.iter().map(Vec::len).chain([header.len()]).max().unwrap_or(0);
    Block::Table {
        aligns: vec![Align::None; ncols],
        header,
        rows,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_org() {
        let doc = parse(
            "#+TITLE: Demo\n\n* Intro :tag:\nSome *bold* and /italic/ text, [[https://x.dev][site]].\n\n- one\n- [X] two\n  - nested\n\n#+BEGIN_SRC rust\nfn main() {}\n#+END_SRC\n\n| a | b |\n|---+---|\n| 1 | 2 |\n",
        );
        assert_eq!(doc.front_matter.as_deref(), Some("title: Demo"));
        assert!(matches!(&doc.blocks[0], Block::Heading { level: 1, id, .. } if id == "intro"));
        let Block::Paragraph(p) = &doc.blocks[1] else { panic!() };
        assert!(p.iter().any(|i| matches!(i, Inline::Strong(_))));
        assert!(p.iter().any(|i| matches!(i, Inline::Link { url, .. } if url == "https://x.dev")));
        let Block::List { items, .. } = &doc.blocks[2] else { panic!() };
        assert_eq!(items[1].task, Some(true));
        assert!(matches!(items[1].blocks[1], Block::List { .. }));
        assert!(matches!(&doc.blocks[3], Block::CodeBlock { lang: Some(l), .. } if l == "rust"));
        assert!(matches!(&doc.blocks[4], Block::Table { header, rows, .. } if header.len() == 2 && rows.len() == 1));
    }
}
