//! reStructuredText parser (common subset).

use std::collections::HashMap;

use super::ast::*;
use super::inline::{self, Kind, Syntax};
use super::lines::*;

thread_local! {
    /// Named hyperlink targets (`.. _name: url`) of the document being parsed.
    static TARGETS: std::cell::RefCell<HashMap<String, String>> = Default::default();
}

const SYNTAX: Syntax = Syntax {
    delims: &[
        ("**", Kind::Strong),
        ("``", Kind::Code),
        ("*", Kind::Emph),
        (":code:`", Kind::Code),
    ],
    special,
};

/// `` `text <url>`_ ``, `` `name`_ ``, `name_`, `:role:`text``.
fn special(rest: &str) -> Option<(Inline, usize)> {
    if rest.starts_with("``") {
        return None;
    }
    if let Some(body) = rest.strip_prefix('`') {
        let end = body.find('`')?;
        let inner = &body[..end];
        let after = &body[end + 1..];
        let underscores = after.chars().take_while(|&c| c == '_').count();
        if underscores == 0 {
            // Interpreted text: show as code-ish emphasis.
            return Some((Inline::Emph(vec![Inline::Text(inner.to_string())]), end + 2));
        }
        let used = 1 + end + 1 + underscores;
        if let Some(open) = inner.rfind('<')
            && inner.ends_with('>')
        {
            let text = inner[..open].trim();
            let url = &inner[open + 1..inner.len() - 1];
            let text = if text.is_empty() { url } else { text };
            return Some((
                Inline::Link {
                    url: url.to_string(),
                    content: vec![Inline::Text(text.to_string())],
                },
                used,
            ));
        }
        return Some((named_ref(inner), used));
    }
    // Roles like :ref:`x` or :doc:`x`.
    if let Some(body) = rest.strip_prefix(':') {
        let name_end = body.find(':')?;
        let name = &body[..name_end];
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            return None;
        }
        let tail = body[name_end + 1..].strip_prefix('`')?;
        let end = tail.find('`')?;
        let text = &tail[..end];
        let text = text.rsplit_once('<').map_or(text, |(t, _)| t.trim());
        let used = 1 + name_end + 1 + 1 + end + 1;
        return Some((
            match name {
                "code" | "literal" => Inline::Code(text.to_string()),
                "math" => Inline::Math(text.to_string()),
                "strong" => Inline::Strong(vec![Inline::Text(text.to_string())]),
                _ => Inline::Emph(vec![Inline::Text(text.to_string())]),
            },
            used,
        ));
    }
    None
}

fn named_ref(name: &str) -> Inline {
    let key = name.trim().to_lowercase();
    let url = TARGETS
        .with(|t| t.borrow().get(&key).cloned())
        .unwrap_or_else(|| format!("#{}", super::markdown::slugify(name)));
    Inline::Link {
        url,
        content: vec![Inline::Text(name.to_string())],
    }
}

pub fn parse(src: &str) -> Document {
    let lines: Vec<&str> = src.lines().collect();
    let targets = lines
        .iter()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix(".. _")?;
            let (name, url) = rest.split_once(": ")?;
            Some((
                name.trim_matches('`').to_lowercase(),
                url.trim().to_string(),
            ))
        })
        .collect();
    TARGETS.with(|t| *t.borrow_mut() = targets);

    let mut p = RstParser {
        slugs: Slugger::default(),
        styles: Vec::new(),
        front: Vec::new(),
    };
    let blocks = p.blocks(&lines, true);
    Document {
        front_matter: (!p.front.is_empty()).then(|| p.front.join("\n")),
        blocks,
    }
}

fn adornment(line: &str) -> Option<char> {
    let t = line.trim_end();
    let c = t.chars().next()?;
    (t.len() >= 2 && !c.is_alphanumeric() && !c.is_whitespace() && t.chars().all(|x| x == c))
        .then_some(c)
}

fn marker(line: &str) -> Option<Marker> {
    if let Some(m) = simple_marker(line, &['-', '*', '+', '•'], true) {
        return Some(m);
    }
    // `#.` auto-numbered items.
    let ind = indent(line);
    let rest = line[ind..].strip_prefix("#. ")?;
    Some(Marker {
        indent: ind,
        content: line.len() - rest.len(),
        ordered: Some(1),
    })
}

fn admonition(name: &str) -> Option<Admonition> {
    Some(match name {
        "note" | "seealso" => Admonition::Note,
        "tip" | "hint" => Admonition::Tip,
        "important" | "attention" => Admonition::Important,
        "warning" => Admonition::Warning,
        "caution" | "danger" | "error" => Admonition::Caution,
        _ => return None,
    })
}

struct RstParser {
    slugs: Slugger,
    /// Heading adornment styles in order of first use (char, has overline).
    styles: Vec<(char, bool)>,
    front: Vec<String>,
}

impl RstParser {
    fn heading(&mut self, style: (char, bool), text: &str) -> Block {
        let level = match self.styles.iter().position(|s| *s == style) {
            Some(p) => p,
            None => {
                self.styles.push(style);
                self.styles.len() - 1
            }
        };
        self.slugs.heading(
            (level + 1).min(6) as u8,
            inline::parse(text.trim(), &SYNTAX),
        )
    }

    fn blocks(&mut self, lines: &[&str], top: bool) -> Vec<Block> {
        let mut out = Vec::new();
        let mut i = 0;
        let mut literal_next = false;
        while i < lines.len() {
            let line = lines[i];
            if is_blank(line) {
                i += 1;
                continue;
            }
            let ind = indent(line);

            // Indented block: literal after `::`, otherwise a block quote.
            if ind > 0 {
                let n = indented_len(&lines[i..], 0);
                let body = dedent(&lines[i..i + n], ind);
                if literal_next {
                    out.push(Block::CodeBlock {
                        lang: None,
                        code: join(trim_blank(&body)),
                    });
                } else {
                    out.push(Block::BlockQuote {
                        kind: None,
                        blocks: self.blocks(&body, false),
                    });
                }
                literal_next = false;
                i += n;
                continue;
            }
            literal_next = false;

            // Over+underlined title.
            if let (Some(c), Some(text), Some(under)) =
                (adornment(line), lines.get(i + 1), lines.get(i + 2))
                && !is_blank(text)
                && adornment(under) == Some(c)
            {
                out.push(self.heading((c, true), text));
                i += 3;
                continue;
            }
            // Underlined title.
            if let Some(under) = lines.get(i + 1)
                && let Some(c) = adornment(under)
                && adornment(line).is_none()
                && under.trim_end().len() >= line.trim().chars().count().min(3)
            {
                out.push(self.heading((c, false), line));
                i += 2;
                continue;
            }
            // Transition.
            if adornment(line).is_some_and(|c| c == '-' || c == '=' || c == '*')
                && line.trim().len() >= 4
            {
                out.push(Block::Rule);
                i += 1;
                continue;
            }
            if let Some(rest) = line.strip_prefix(".. ") {
                i += self.directive(rest, &lines[i + 1..], &mut out) + 1;
                continue;
            }
            if line.starts_with("..") && line.trim() == ".." {
                i += 1 + indented_len(&lines[i + 1..], 1);
                continue;
            }
            // Field list at the top: document info.
            if top
                && out.iter().all(|b| matches!(b, Block::Heading { .. }))
                && let Some(field) = line.strip_prefix(':')
                && let Some((name, value)) =
                    field.split_once(": ").or_else(|| field.split_once(':'))
            {
                self.front
                    .push(format!("{}: {}", name.to_lowercase(), value.trim()));
                i += 1;
                continue;
            }
            if line.starts_with('+') && line.trim_end().ends_with('+') || line.starts_with("===") {
                // Grid or simple table: keep the ASCII art as-is.
                let n = lines[i..].iter().take_while(|l| !is_blank(l)).count();
                out.push(Block::CodeBlock {
                    lang: None,
                    code: join(&lines[i..i + n]),
                });
                i += n;
                continue;
            }
            if marker(line).is_some() {
                let (list, used) = collect_list(&lines[i..], marker, |b| self.blocks(b, false));
                out.push(list);
                i += used;
                continue;
            }
            let n = lines[i..]
                .iter()
                .take_while(|l| !is_blank(l) && indent(l) == 0)
                .count()
                .max(1);
            // A following adornment line belongs to a heading, not this paragraph.
            let n = (1..n)
                .find(|&k| adornment(lines[i + k]).is_some())
                .map_or(n, |k| k - 1)
                .max(1);
            let mut para: Vec<String> = lines[i..i + n].iter().map(|s| s.to_string()).collect();
            if let Some(last) = para.last_mut()
                && last.trim_end().ends_with("::")
            {
                literal_next = true;
                let t = last.trim_end();
                *last = if t == "::" {
                    String::new()
                } else if let Some(text) = t.strip_suffix(" ::") {
                    text.to_string()
                } else {
                    t[..t.len() - 1].to_string()
                };
            }
            let refs: Vec<&str> = para
                .iter()
                .map(String::as_str)
                .filter(|s| !s.is_empty())
                .collect();
            if !refs.is_empty() {
                out.push(Block::Paragraph(inline::paragraph(&refs, &SYNTAX)));
            }
            i += n;
        }
        out
    }

    /// Handles `.. name:: args` and returns the number of extra lines consumed.
    fn directive(&mut self, rest: &str, following: &[&str], out: &mut Vec<Block>) -> usize {
        let n = indented_len(following, 1);
        let body_lines = &following[..n];
        let base = body_lines
            .iter()
            .filter(|l| !is_blank(l))
            .map(|l| indent(l))
            .min()
            .unwrap_or(0);
        let body = dedent(body_lines, base);
        // Skip directive options (`:linenos:` etc.).
        let content_start = body
            .iter()
            .position(|l| !l.starts_with(':') && !is_blank(l))
            .unwrap_or(body.len());
        let content = trim_blank(&body[content_start..]);

        let Some((name, arg)) = rest.split_once("::") else {
            // Comment or hyperlink target.
            return n;
        };
        let name = name.trim().to_lowercase();
        let arg = arg.trim();
        match name.as_str() {
            "code-block" | "code" | "sourcecode" => out.push(Block::CodeBlock {
                lang: arg.split_whitespace().next().map(str::to_string),
                code: join(content),
            }),
            "image" | "figure" => {
                out.push(Block::Paragraph(vec![Inline::Image {
                    url: arg.to_string(),
                    alt: body
                        .iter()
                        .find_map(|l| l.trim().strip_prefix(":alt:"))
                        .unwrap_or("")
                        .trim()
                        .to_string(),
                }]));
                if name == "figure" && !content.is_empty() {
                    out.extend(self.blocks(content, false));
                }
            }
            "math" => out.push(Block::Math(
                std::iter::once(arg)
                    .chain(content.iter().copied())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )),
            "admonition" => {
                let mut blocks = vec![Block::Plain(vec![Inline::Strong(vec![Inline::Text(
                    arg.to_string(),
                )])])];
                blocks.extend(self.blocks(content, false));
                out.push(Block::BlockQuote { kind: None, blocks });
            }
            "contents" | "toctree" | "index" | "meta" | "raw" | "include" | "only" => {}
            other => {
                if let Some(kind) = admonition(other) {
                    let mut lines: Vec<&str> = Vec::new();
                    if !arg.is_empty() {
                        lines.push(arg);
                    }
                    lines.extend_from_slice(content);
                    out.push(Block::BlockQuote {
                        kind: Some(kind),
                        blocks: self.blocks(&lines, false),
                    });
                } else if !content.is_empty() {
                    out.extend(self.blocks(content, false));
                }
            }
        }
        n
    }
}

/// Number of following lines that are blank or indented at least `min`.
fn indented_len(lines: &[&str], min: usize) -> usize {
    let mut n = 0;
    let mut last_content = 0;
    while n < lines.len() && (is_blank(lines[n]) || indent(lines[n]) >= min.max(1)) {
        n += 1;
        if !is_blank(lines[n - 1]) {
            last_content = n;
        }
    }
    last_content
}

fn trim_blank<'a, 'b>(lines: &'b [&'a str]) -> &'b [&'a str] {
    let start = lines
        .iter()
        .position(|l| !is_blank(l))
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !is_blank(l))
        .map_or(start, |e| e + 1);
    &lines[start..end]
}

fn join(lines: &[&str]) -> String {
    let mut s = lines.join("\n");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rst() {
        let doc = parse(
            "=====\nTitle\n=====\n\nSection\n-------\n\nSome **bold** and *em* with `link <https://x.dev>`_ and ``code``.\n\n- one\n- two\n\n  - nested\n\n.. note:: Remember this.\n\n.. code-block:: python\n\n   print(1)\n\nExample::\n\n    raw text\n",
        );
        assert!(matches!(&doc.blocks[0], Block::Heading { level: 1, .. }));
        assert!(matches!(&doc.blocks[1], Block::Heading { level: 2, id, .. } if id == "section"));
        let Block::Paragraph(p) = &doc.blocks[2] else {
            panic!("{:?}", doc.blocks[2])
        };
        assert!(
            p.iter()
                .any(|i| matches!(i, Inline::Link { url, .. } if url == "https://x.dev"))
        );
        assert!(
            p.iter()
                .any(|i| matches!(i, Inline::Code(c) if c == "code"))
        );
        let Block::List { items, .. } = &doc.blocks[3] else {
            panic!("{:?}", doc.blocks[3])
        };
        assert_eq!(items.len(), 2);
        assert!(matches!(
            &doc.blocks[4],
            Block::BlockQuote {
                kind: Some(Admonition::Note),
                ..
            }
        ));
        assert!(
            matches!(&doc.blocks[5], Block::CodeBlock { lang: Some(l), code } if l == "python" && code == "print(1)\n")
        );
        assert!(matches!(&doc.blocks[6], Block::Paragraph(_)));
        assert!(matches!(&doc.blocks[7], Block::CodeBlock { code, .. } if code == "raw text\n"));
    }
}
