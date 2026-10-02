//! AsciiDoc parser (common subset).

use super::ast::*;
use super::inline::{self, Kind, Syntax};
use super::lines::*;

const SYNTAX: Syntax = Syntax {
    delims: &[
        ("**", Kind::Strong),
        ("__", Kind::Emph),
        ("``", Kind::Code),
        ("*", Kind::Strong),
        ("_", Kind::Emph),
        ("`", Kind::Code),
        ("#", Kind::Emph),
    ],
    special,
};

/// Macros: `link:url[text]`, `https://url[text]`, `image:url[alt]`, `<<id,text>>`.
fn special(rest: &str) -> Option<(Inline, usize)> {
    if let Some(body) = rest.strip_prefix("<<") {
        let end = body.find(">>")?;
        let (id, text) = match body[..end].split_once(',') {
            Some((id, t)) => (id.trim(), t.trim()),
            None => (body[..end].trim(), body[..end].trim()),
        };
        return Some((
            Inline::Link {
                url: format!("#{id}"),
                content: vec![Inline::Text(text.to_string())],
            },
            2 + end + 2,
        ));
    }
    let (kind, target_start) = if let Some(r) = rest.strip_prefix("image:") {
        ("image", rest.len() - r.len())
    } else if let Some(r) = rest.strip_prefix("link:") {
        ("link", rest.len() - r.len())
    } else if rest.starts_with("http://") || rest.starts_with("https://") || rest.starts_with("mailto:") {
        ("link", 0)
    } else {
        return None;
    };
    let after = &rest[target_start..];
    let open = after.find(|c: char| c == '[' || c.is_whitespace())?;
    if !after[open..].starts_with('[') {
        return None;
    }
    let close = after[open..].find(']')? + open;
    let target = &after[..open];
    let text = &after[open + 1..close];
    let used = target_start + close + 1;
    if kind == "image" {
        let alt = text.split(',').next().unwrap_or("").to_string();
        return Some((Inline::Image { url: target.to_string(), alt }, used));
    }
    let text = text.trim_end_matches('^');
    let content = if text.is_empty() {
        vec![Inline::Text(target.to_string())]
    } else {
        inline::parse(text, &SYNTAX)
    };
    Some((
        Inline::Link {
            url: target.to_string(),
            content,
        },
        used,
    ))
}

pub fn parse(src: &str) -> Document {
    let lines: Vec<&str> = src.lines().collect();
    let mut p = AdocParser {
        slugs: Slugger::default(),
        front: Vec::new(),
    };
    let blocks = p.blocks(&lines, true);
    Document {
        front_matter: (!p.front.is_empty()).then(|| p.front.join("\n")),
        blocks,
    }
}

fn heading(line: &str) -> Option<(u8, &str)> {
    let marks = line.chars().take_while(|&c| c == '=' || c == '#').count();
    if marks == 0 || marks > 6 || !line[marks..].starts_with(' ') {
        return None;
    }
    Some((marks as u8, line[marks..].trim()))
}

/// `:name: value` document attribute.
fn attribute(line: &str) -> Option<(&str, &str)> {
    let rest = line.strip_prefix(':')?;
    let (name, value) = rest.split_once(':')?;
    (!name.is_empty() && !name.contains(' ')).then(|| (name, value.trim()))
}

/// Depth-based list marker: `*`, `**`, `-`, `.`, `..`.
fn list_marker(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start();
    let c = t.chars().next()?;
    if !matches!(c, '*' | '-' | '.') {
        return None;
    }
    let depth = t.chars().take_while(|&x| x == c).count();
    let rest = &t[depth..];
    if !rest.starts_with(' ') || (c == '-' && depth > 1) {
        return None;
    }
    Some((c, depth, rest.trim_start()))
}

fn is_delimiter(t: &str) -> Option<char> {
    let c = t.chars().next()?;
    (t.len() >= 4 && matches!(c, '-' | '.' | '_' | '=' | '*' | '+') && t.chars().all(|x| x == c))
        .then_some(c)
}

const ADMONITIONS: &[(&str, Admonition)] = &[
    ("NOTE", Admonition::Note),
    ("TIP", Admonition::Tip),
    ("IMPORTANT", Admonition::Important),
    ("WARNING", Admonition::Warning),
    ("CAUTION", Admonition::Caution),
];

struct AdocParser {
    slugs: Slugger,
    front: Vec<String>,
}

impl AdocParser {
    fn blocks(&mut self, lines: &[&str], top: bool) -> Vec<Block> {
        let mut out = Vec::new();
        let mut i = 0;
        let mut pending_attr: Option<String> = None;
        let mut header = top;
        while i < lines.len() {
            let line = lines[i];
            let t = line.trim();
            if t.is_empty() {
                header = false;
                i += 1;
                continue;
            }
            if t.starts_with("//") && !t.starts_with("////") {
                i += 1;
                continue;
            }
            if let Some((name, value)) = attribute(t) {
                if header || top && out.len() <= 1 {
                    self.front.push(format!("{name}: {value}"));
                }
                i += 1;
                continue;
            }
            if t.starts_with('[') && t.ends_with(']') && !t.starts_with("[[") {
                pending_attr = Some(t[1..t.len() - 1].to_string());
                i += 1;
                continue;
            }
            if let Some((level, text)) = heading(line) {
                out.push(self.slugs.heading(level, inline::parse(text, &SYNTAX)));
                i += 1;
                continue;
            }
            if t == "'''" || t == "---" || t == "***" {
                out.push(Block::Rule);
                i += 1;
                continue;
            }
            if t.starts_with('.') && !t.starts_with("..") && t.len() > 1 && !t[1..].starts_with(' ') {
                // Block title.
                out.push(Block::Plain(vec![Inline::Strong(inline::parse(&t[1..], &SYNTAX))]));
                i += 1;
                continue;
            }
            if t.starts_with("|===") {
                let end = lines[i + 1..]
                    .iter()
                    .position(|l| l.trim().starts_with("|==="))
                    .map_or(lines.len(), |p| i + 1 + p);
                out.push(table(&lines[i + 1..end], pending_attr.take()));
                i = end + 1;
                continue;
            }
            if let Some(c) = is_delimiter(t) {
                let end = lines[i + 1..]
                    .iter()
                    .position(|l| l.trim() == t)
                    .map_or(lines.len(), |p| i + 1 + p);
                let body = &lines[i + 1..end];
                let attr = pending_attr.take().unwrap_or_default();
                let mut parts = attr.split(',').map(str::trim);
                let style = parts.next().unwrap_or("");
                out.push(match c {
                    '-' | '.' | '+' => Block::CodeBlock {
                        lang: if style == "source" {
                            parts.next().map(str::to_string)
                        } else {
                            None
                        },
                        code: join(body),
                    },
                    '/' => {
                        i = end + 1;
                        continue;
                    }
                    _ => {
                        let kind = ADMONITIONS.iter().find(|(n, _)| *n == style).map(|(_, k)| *k);
                        Block::BlockQuote {
                            kind,
                            blocks: self.blocks(body, false),
                        }
                    }
                });
                i = end + 1;
                continue;
            }
            if t.starts_with("////") {
                let end = lines[i + 1..]
                    .iter()
                    .position(|l| l.trim() == t)
                    .map_or(lines.len(), |p| i + 1 + p);
                i = end + 1;
                continue;
            }
            if let Some(rest) = t.strip_prefix("image::") {
                if let Some((Inline::Image { url, alt }, _)) = special(&format!("image:{rest}")) {
                    out.push(Block::Paragraph(vec![Inline::Image { url, alt }]));
                }
                i += 1;
                continue;
            }
            if list_marker(line).is_some() {
                let (list, used) = self.list(&lines[i..]);
                out.push(list);
                i += used;
                continue;
            }
            let n = lines[i..]
                .iter()
                .take_while(|l| !is_blank(l) && list_marker(l).is_none() && heading(l).is_none())
                .count()
                .max(1);
            let para = &lines[i..i + n];
            let admonition = ADMONITIONS.iter().find_map(|(name, k)| {
                para[0].trim().strip_prefix(name)?.strip_prefix(": ").map(|r| (k, r))
            });
            match admonition {
                Some((k, first)) => {
                    let mut body = vec![first];
                    body.extend_from_slice(&para[1..]);
                    out.push(Block::BlockQuote {
                        kind: Some(*k),
                        blocks: vec![Block::Paragraph(inline::paragraph(&body, &SYNTAX))],
                    });
                }
                None if line.starts_with(' ') => out.push(Block::CodeBlock {
                    lang: None,
                    code: join(&dedent(para, indent(para[0]))),
                }),
                None => out.push(Block::Paragraph(inline::paragraph(para, &SYNTAX))),
            }
            i += n;
        }
        out
    }

    /// Lists nest by marker depth (`*`, `**`, ...), not indentation.
    fn list(&mut self, lines: &[&str]) -> (Block, usize) {
        let (c, depth, _) = list_marker(lines[0]).expect("checked");
        let mut items: Vec<ListItem> = Vec::new();
        let mut i = 0;
        let mut after_blank = false;
        while i < lines.len() {
            if is_blank(lines[i]) {
                // A blank line ends the list unless another item follows.
                if lines.get(i + 1).and_then(|l| list_marker(l)).is_some() {
                    after_blank = true;
                    i += 1;
                    continue;
                }
                break;
            }
            let Some((mc, md, text)) = list_marker(lines[i]) else {
                // `+` attaches the next block; plain lines continue the item.
                if lines[i].trim() == "+" {
                    i += 1;
                    continue;
                }
                if let Some(Block::Plain(inl)) = items.last_mut().and_then(|it| it.blocks.first_mut()) {
                    inl.push(Inline::SoftBreak);
                    inl.extend(inline::parse(lines[i].trim(), &SYNTAX));
                    i += 1;
                    continue;
                }
                break;
            };
            if mc != c && after_blank && depth == 1 {
                // A different list type after a blank line starts a new list.
                break;
            }
            after_blank = false;
            if md > depth || mc != c {
                let (nested, used) = self.list(&lines[i..]);
                match items.last_mut() {
                    Some(it) => it.blocks.push(nested),
                    None => items.push(ListItem {
                        task: None,
                        blocks: vec![nested],
                    }),
                }
                i += used;
                continue;
            }
            if md < depth {
                break;
            }
            let (task, text) = match text.get(..4) {
                Some("[ ] ") => (Some(false), &text[4..]),
                Some("[x] ") | Some("[X] ") | Some("[*] ") => (Some(true), &text[4..]),
                _ => (None, text),
            };
            items.push(ListItem {
                task,
                blocks: vec![Block::Plain(inline::parse(text, &SYNTAX))],
            });
            i += 1;
        }
        (
            Block::List {
                start: (c == '.').then_some(1),
                tight: true,
                items,
            },
            i.max(1),
        )
    }
}

fn join(lines: &[&str]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

fn table(lines: &[&str], attr: Option<String>) -> Block {
    // Rows are separated by blank lines or each starts with `|`.
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut header_by_blank = false;
    let mut cur: Vec<String> = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        let t = l.trim();
        if t.is_empty() {
            if !cur.is_empty() {
                if rows.is_empty() && n > 0 {
                    header_by_blank = true;
                }
                rows.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if t.starts_with('|') {
            let cells: Vec<String> = t[1..].split('|').map(|c| c.trim().to_string()).collect();
            // A full row on one line ends the row.
            if cur.is_empty() && rows.first().is_some_and(|r| r.len() == cells.len()) {
                rows.push(cells);
            } else {
                cur.extend(cells);
                let width = rows.first().map_or(usize::MAX, Vec::len);
                if cur.len() >= width {
                    rows.push(std::mem::take(&mut cur));
                }
            }
        } else if let Some(last) = cur.last_mut() {
            last.push(' ');
            last.push_str(t);
        }
    }
    if !cur.is_empty() {
        rows.push(cur);
    }
    let header_attr = attr.as_deref().is_some_and(|a| a.contains("header"));
    let to_inl = |r: Vec<String>| -> Vec<Vec<Inline>> {
        r.iter().map(|c| inline::parse(c, &SYNTAX)).collect()
    };
    let mut rows: Vec<_> = rows.into_iter().map(to_inl).collect();
    let header = if (header_attr || header_by_blank) && !rows.is_empty() {
        rows.remove(0)
    } else {
        Vec::new()
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
    fn parses_asciidoc() {
        let doc = parse(
            "= Title\n:author: Me\n\n== Section\n\nSome *bold* and _em_ text, https://x.dev[site].\n\n* one\n** nested\n* [x] two\n\nNOTE: Remember this.\n\n[source,rust]\n----\nfn main() {}\n----\n\n|===\n| A | B\n\n| 1 | 2\n|===\n",
        );
        assert_eq!(doc.front_matter.as_deref(), Some("author: Me"));
        assert!(matches!(&doc.blocks[0], Block::Heading { level: 1, .. }));
        assert!(matches!(&doc.blocks[1], Block::Heading { level: 2, id, .. } if id == "section"));
        let Block::Paragraph(p) = &doc.blocks[2] else { panic!("{:?}", doc.blocks[2]) };
        assert!(p.iter().any(|i| matches!(i, Inline::Link { url, .. } if url == "https://x.dev")));
        let Block::List { items, .. } = &doc.blocks[3] else { panic!() };
        assert_eq!(items.len(), 2);
        assert!(matches!(items[0].blocks[1], Block::List { .. }));
        assert_eq!(items[1].task, Some(true));
        assert!(matches!(&doc.blocks[4], Block::BlockQuote { kind: Some(Admonition::Note), .. }));
        assert!(matches!(&doc.blocks[5], Block::CodeBlock { lang: Some(l), .. } if l == "rust"));
        assert!(matches!(&doc.blocks[6], Block::Table { header, rows, .. } if header.len() == 2 && rows.len() == 1));
    }
}
