//! Turns a `Document` into width-bounded styled lines.

mod table;
pub mod wrap;

use ratatui::style::{Modifier, Style};
use unicode_width::UnicodeWidthStr;

use crate::parser::{Admonition, Block, Document, Inline, ListItem, plain_text};
use crate::theme::Theme;
use wrap::{Seg, push_span, wrap};

#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: Style,
    /// Index into `Rendered::links`.
    pub link: Option<usize>,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Span {
            text: text.into(),
            style,
            link: None,
        }
    }

    pub fn with_text(&self, text: &str) -> Self {
        Span {
            text: text.to_string(),
            style: self.style,
            link: self.link,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    pub spans: Vec<Span>,
    /// Anchor target defined on this line (heading id, `^footnote`).
    pub anchor: Option<String>,
    /// Index into `Rendered::headings`.
    pub heading: Option<usize>,
    /// Index into `Rendered::code_blocks`.
    pub code: Option<usize>,
}

impl Line {
    fn new(spans: Vec<Span>) -> Self {
        Line {
            spans,
            ..Default::default()
        }
    }

    fn blank() -> Self {
        Line::default()
    }

    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HeadingEntry {
    pub level: u8,
    pub title: String,
    pub id: String,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CodeEntry {
    pub lang: Option<String>,
    pub code: String,
    pub line: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Rendered {
    pub lines: Vec<Line>,
    pub headings: Vec<HeadingEntry>,
    pub links: Vec<String>,
    pub code_blocks: Vec<CodeEntry>,
}

impl Rendered {
    pub fn anchor_line(&self, id: &str) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| l.anchor.as_deref() == Some(id))
    }
}

#[derive(Debug, Clone)]
pub struct Options {
    pub width: usize,
    /// Show `[n]` on code blocks so they can be copied by number.
    pub code_numbers: bool,
    /// Render front matter as a block at the top.
    pub front_matter: bool,
}

pub fn layout(doc: &Document, theme: &Theme, opts: &Options) -> Rendered {
    let mut l = Layouter {
        theme,
        opts,
        links: Vec::new(),
        headings: Vec::new(),
        codes: Vec::new(),
        list_depth: 0,
    };
    let width = opts.width.max(20);
    let mut lines = Vec::new();
    if opts.front_matter
        && let Some(fm) = &doc.front_matter
    {
        lines.extend(l.front_matter(fm, width));
        lines.push(Line::blank());
    }
    lines.extend(l.blocks(&doc.blocks, width));
    while lines.last().is_some_and(|ln| ln.spans.is_empty()) {
        lines.pop();
    }

    let mut headings = l.headings;
    let mut code_blocks = l.codes;
    for (n, line) in lines.iter().enumerate() {
        if let Some(h) = line.heading
            && headings[h].line == usize::MAX
        {
            headings[h].line = n;
        }
        if let Some(c) = line.code
            && code_blocks[c].line == usize::MAX
        {
            code_blocks[c].line = n;
        }
    }
    Rendered {
        lines,
        headings,
        links: l.links,
        code_blocks,
    }
}

struct Layouter<'a> {
    theme: &'a Theme,
    opts: &'a Options,
    links: Vec<String>,
    headings: Vec<HeadingEntry>,
    codes: Vec<CodeEntry>,
    list_depth: usize,
}

impl Layouter<'_> {
    fn blocks(&mut self, blocks: &[Block], width: usize) -> Vec<Line> {
        let mut out = Vec::new();
        for (i, b) in blocks.iter().enumerate() {
            if i > 0 && !matches!(blocks[i - 1], Block::Plain(_)) {
                out.push(Line::blank());
            }
            out.extend(self.block(b, width));
        }
        out
    }

    fn block(&mut self, block: &Block, width: usize) -> Vec<Line> {
        let t = self.theme;
        match block {
            Block::Paragraph(inl) | Block::Plain(inl) => self.paragraph(inl, t.text, width),
            Block::Heading { level, id, content } => self.heading(*level, id, content, width),
            Block::CodeBlock { lang, code } => self.code_block(lang.as_deref(), code, width),
            Block::BlockQuote { kind, blocks } => self.quote(*kind, blocks, width),
            Block::List {
                start,
                tight,
                items,
            } => self.list(*start, *tight, items, width),
            Block::Table {
                aligns,
                header,
                rows,
            } => self.table(aligns, header, rows, width),
            Block::DefinitionList(entries) => {
                let mut out = Vec::new();
                for (n, (term, defs)) in entries.iter().enumerate() {
                    if n > 0 {
                        out.push(Line::blank());
                    }
                    out.extend(self.paragraph(term, t.text.patch(t.strong), width));
                    for d in defs {
                        let inner = self.blocks(d, width.saturating_sub(4));
                        let marker = vec![Span::new("  : ", t.bullet)];
                        out.extend(prefix(inner, marker, vec![Span::new("    ", t.text)]));
                    }
                }
                out
            }
            Block::FootnoteDef { label, blocks } => {
                let mark = format!("[{label}] ");
                let w = mark.width();
                let inner = self.blocks(blocks, width.saturating_sub(w));
                let mut out = prefix(
                    inner,
                    vec![Span::new(mark, t.footnote)],
                    vec![Span::new(" ".repeat(w), t.text)],
                );
                if let Some(first) = out.first_mut() {
                    first.anchor = Some(format!("^{label}"));
                }
                out
            }
            Block::Math(m) => crate::math::to_unicode(m)
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .flat_map(|l| {
                    wrap::hard_wrap(vec![Span::new(l, t.math)], width.saturating_sub(4))
                        .into_iter()
                        .map(|row| {
                            let mut spans = vec![Span::new("    ", t.math)];
                            spans.extend(row);
                            Line::new(spans)
                        })
                })
                .collect(),
            Block::Html(html) => {
                let text = strip_html(html);
                if text.trim().is_empty() {
                    return Vec::new();
                }
                text.lines()
                    .flat_map(|l| self.paragraph(&[Inline::Text(l.trim().to_string())], t.dim, width))
                    .collect()
            }
            Block::Rule => vec![Line::new(vec![Span::new("─".repeat(width), t.rule)])],
        }
    }

    fn front_matter(&mut self, fm: &str, width: usize) -> Vec<Line> {
        let t = self.theme;
        let b = t.code_border;
        let inner = width.saturating_sub(4).max(1);
        let title = " front matter ";
        let mut out = vec![Line::new(vec![
            Span::new("╭─", b),
            Span::new(title, t.dim),
            Span::new(format!("{}╮", "─".repeat(width.saturating_sub(3 + title.width()))), b),
        ])];
        for src in fm.lines() {
            let spans = match src.split_once(':') {
                Some((k, v)) => vec![
                    Span::new(format!("{k}:"), t.code_lang),
                    Span::new(v.to_string(), t.dim),
                ],
                None => vec![Span::new(src.to_string(), t.dim)],
            };
            for row in wrap::hard_wrap(spans, inner) {
                let pad = inner.saturating_sub(wrap::width(&row));
                let mut spans = vec![Span::new("│ ", b)];
                spans.extend(row);
                spans.push(Span::new(format!("{} │", " ".repeat(pad)), b));
                out.push(Line::new(spans));
            }
        }
        out.push(Line::new(vec![Span::new(
            format!("╰{}╯", "─".repeat(width.saturating_sub(2))),
            b,
        )]));
        out
    }

    fn paragraph(&mut self, inl: &[Inline], base: Style, width: usize) -> Vec<Line> {
        let mut segs = Vec::new();
        self.flatten(inl, base, None, &mut segs);
        wrap(segs, width).into_iter().map(Line::new).collect()
    }

    fn heading(&mut self, level: u8, id: &str, content: &[Inline], width: usize) -> Vec<Line> {
        let t = self.theme;
        let style = t.headings[(level as usize).saturating_sub(1).min(5)];
        let marker = match level {
            1 | 2 => "",
            3 => "▸ ",
            4 => "▹ ",
            5 => "• ",
            _ => "· ",
        };
        let mw = marker.width();
        let mut segs = Vec::new();
        self.flatten(content, style, None, &mut segs);
        let lines = wrap(segs, width.saturating_sub(mw));
        let mut out = prefix(
            lines.into_iter().map(Line::new).collect(),
            vec![Span::new(marker, style)],
            vec![Span::new(" ".repeat(mw), style)],
        );
        match level {
            1 => out.push(Line::new(vec![Span::new("━".repeat(width), style)])),
            2 => out.push(Line::new(vec![Span::new(
                "─".repeat(width),
                style.remove_modifier(Modifier::BOLD),
            )])),
            _ => {}
        }
        let idx = self.headings.len();
        self.headings.push(HeadingEntry {
            level,
            title: plain_text(content),
            id: id.to_string(),
            line: usize::MAX,
        });
        for l in &mut out {
            l.heading = Some(idx);
        }
        out[0].anchor = Some(id.to_string());
        out
    }

    fn code_block(&mut self, lang: Option<&str>, code: &str, width: usize) -> Vec<Line> {
        let t = self.theme;
        let idx = self.codes.len();
        self.codes.push(CodeEntry {
            lang: lang.map(str::to_string),
            code: code.to_string(),
            line: usize::MAX,
        });
        let inner = width.saturating_sub(4).max(1);
        let b = t.code_border;

        let mut top = vec![Span::new("╭─", b)];
        let mut used = 2;
        if let Some(lang) = lang {
            top.push(Span::new(format!(" {lang} "), t.code_lang));
            used += lang.width() + 2;
        }
        let num = if self.opts.code_numbers {
            format!(" [{}] ", idx + 1)
        } else {
            String::new()
        };
        let fill = width.saturating_sub(used + num.width() + 1);
        top.push(Span::new("─".repeat(fill), b));
        if !num.is_empty() {
            top.push(Span::new(num, t.dim));
        }
        top.push(Span::new("╮", b));

        let mut out = vec![Line::new(top)];
        let code = code.strip_suffix('\n').unwrap_or(code).replace('\t', "    ");
        for src_line in crate::highlight::highlight(&code, lang, t) {
            for row in wrap::hard_wrap(src_line, inner) {
                let pad = inner.saturating_sub(wrap::width(&row));
                let mut spans = vec![Span::new("│ ", b)];
                spans.extend(row);
                spans.push(Span::new(format!("{} │", " ".repeat(pad)), b));
                out.push(Line::new(spans));
            }
        }
        out.push(Line::new(vec![Span::new(
            format!("╰{}╯", "─".repeat(width.saturating_sub(2))),
            b,
        )]));
        for l in &mut out {
            l.code = Some(idx);
        }
        out
    }

    fn quote(&mut self, kind: Option<Admonition>, blocks: &[Block], width: usize) -> Vec<Line> {
        let t = self.theme;
        let bar_style = match kind {
            Some(k) => t.admonitions[k as usize].remove_modifier(Modifier::BOLD),
            None => t.quote_bar,
        };
        let mut inner = Vec::new();
        if let Some(k) = kind {
            let (icon, title) = match k {
                Admonition::Note => ("ℹ", "Note"),
                Admonition::Tip => ("💡", "Tip"),
                Admonition::Important => ("❗", "Important"),
                Admonition::Warning => ("⚠", "Warning"),
                Admonition::Caution => ("⛔", "Caution"),
            };
            inner.push(Line::new(vec![Span::new(
                format!("{icon} {title}"),
                t.admonitions[k as usize],
            )]));
        }
        let mut body = self.blocks(blocks, width.saturating_sub(2));
        if kind.is_none() {
            for s in body.iter_mut().flat_map(|l| l.spans.iter_mut()) {
                if s.style == t.text {
                    s.style = t.quote_text;
                }
            }
        }
        inner.extend(body);
        let bar = vec![Span::new("│ ", bar_style)];
        prefix(inner, bar.clone(), bar)
    }

    fn list(&mut self, start: Option<u64>, tight: bool, items: &[ListItem], width: usize) -> Vec<Line> {
        let t = self.theme;
        let bullet = ["•", "◦", "▪", "▫"][self.list_depth % 4];
        let markers: Vec<(String, Style)> = items
            .iter()
            .enumerate()
            .map(|(i, item)| match (item.task, start) {
                (Some(true), _) => ("☑".to_string(), t.task_done),
                (Some(false), _) => ("☐".to_string(), t.task_todo),
                (None, Some(n)) => (format!("{}.", n + i as u64), t.bullet),
                (None, None) => (bullet.to_string(), t.bullet),
            })
            .collect();
        let mw = markers.iter().map(|(m, _)| m.width()).max().unwrap_or(1) + 1;

        self.list_depth += 1;
        let mut out = Vec::new();
        for (i, (item, (marker, mstyle))) in items.iter().zip(markers).enumerate() {
            if i > 0 && !tight {
                out.push(Line::blank());
            }
            let mut inner = self.blocks(&item.blocks, width.saturating_sub(mw));
            if inner.is_empty() {
                inner.push(Line::blank());
            }
            if item.task == Some(true) {
                for l in &mut inner {
                    for s in &mut l.spans {
                        s.style = s.style.patch(t.dim);
                    }
                }
            }
            // Numbers are right-aligned, bullets left-aligned.
            let pad = mw - marker.width();
            let first = if start.is_some() && item.task.is_none() {
                format!("{}{marker} ", " ".repeat(pad - 1))
            } else {
                format!("{marker}{}", " ".repeat(pad))
            };
            out.extend(prefix(
                inner,
                vec![Span::new(first, mstyle)],
                vec![Span::new(" ".repeat(mw), t.text)],
            ));
        }
        self.list_depth -= 1;
        out
    }

    fn flatten(&mut self, inl: &[Inline], base: Style, link: Option<usize>, out: &mut Vec<Seg>) {
        let t = self.theme;
        let span = |text: String, style: Style| {
            Seg::Span(Span {
                text,
                style,
                link,
            })
        };
        for i in inl {
            match i {
                Inline::Text(s) => out.push(span(s.clone(), base)),
                Inline::Code(s) => out.push(span(s.clone(), base.patch(t.code))),
                Inline::Emph(c) => self.flatten(c, base.patch(t.emph), link, out),
                Inline::Strong(c) => self.flatten(c, base.patch(t.strong), link, out),
                Inline::Strike(c) => self.flatten(c, base.patch(t.strike), link, out),
                Inline::Sup(c) => out.push(span(crate::math::superscript(&plain_text(c)), base)),
                Inline::Sub(c) => out.push(span(crate::math::subscript(&plain_text(c)), base)),
                Inline::Link { url, content } => {
                    let id = self.push_link(url);
                    self.flatten(content, base.patch(t.link), Some(id), out);
                }
                Inline::Image { url, alt } => {
                    let id = self.push_link(url);
                    let label = if alt.is_empty() { "image" } else { alt };
                    out.push(Seg::Span(Span {
                        text: format!("[🖼 {label}]"),
                        style: base.patch(t.image),
                        link: Some(id),
                    }));
                }
                Inline::FootnoteRef(label) => {
                    let id = self.push_link(&format!("#^{label}"));
                    out.push(Seg::Span(Span {
                        text: format!("[{label}]"),
                        style: base.patch(t.footnote),
                        link: Some(id),
                    }));
                }
                Inline::Math(m) | Inline::DisplayMath(m) => {
                    out.push(span(crate::math::to_unicode(m), base.patch(t.math)))
                }
                Inline::Html(_) => {}
                Inline::SoftBreak => out.push(Seg::Space(Span {
                    text: " ".into(),
                    style: base,
                    link,
                })),
                Inline::HardBreak => out.push(Seg::Break),
            }
        }
    }

    fn push_link(&mut self, url: &str) -> usize {
        self.links.push(url.to_string());
        self.links.len() - 1
    }
}

/// Prepends `first` to the first line and `rest` to the others.
fn prefix(lines: Vec<Line>, first: Vec<Span>, rest: Vec<Span>) -> Vec<Line> {
    lines
        .into_iter()
        .enumerate()
        .map(|(i, mut l)| {
            let mut spans = if i == 0 { first.clone() } else { rest.clone() };
            for s in l.spans.drain(..) {
                push_span(&mut spans, s);
            }
            l.spans = spans;
            l
        })
        .collect()
}

/// Removes tags and comments from an HTML fragment, keeping text.
pub fn strip_html(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let end = if after.starts_with("<!--") {
            after.find("-->").map(|e| e + 3)
        } else {
            after.find('>').map(|e| e + 1)
        };
        match end {
            Some(e) => rest = &after[e..],
            None => {
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
