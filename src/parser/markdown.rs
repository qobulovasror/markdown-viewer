//! Markdown (CommonMark + GFM) parser built on pulldown-cmark events.

use std::collections::HashMap;

use pulldown_cmark::{
    Alignment, BlockQuoteKind, CodeBlockKind, Event, Options, Parser, Tag, TagEnd,
};

use super::ast::*;

pub fn parse(src: &str) -> Document {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_SUPERSCRIPT
        | Options::ENABLE_SUBSCRIPT;
    let events: Vec<Event> = Parser::new_ext(src, opts).collect();
    let mut b = Builder {
        events,
        pos: 0,
        slugs: HashMap::new(),
        front_matter: None,
    };
    let blocks = b.blocks();
    Document {
        front_matter: b.front_matter,
        blocks,
    }
}

struct Builder<'a> {
    events: Vec<Event<'a>>,
    pos: usize,
    slugs: HashMap<String, usize>,
    front_matter: Option<String>,
}

impl<'a> Builder<'a> {
    fn next(&mut self) -> Option<Event<'a>> {
        let ev = self.events.get(self.pos).cloned();
        self.pos += 1;
        ev
    }

    fn peek(&self) -> Option<&Event<'a>> {
        self.events.get(self.pos)
    }

    /// Reads blocks until the enclosing End event (consumed) or end of input.
    fn blocks(&mut self) -> Vec<Block> {
        let mut out = Vec::new();
        let mut pending: Vec<Inline> = Vec::new();
        while let Some(ev) = self.next() {
            match ev {
                Event::End(_) => break,
                Event::Rule => {
                    flush(&mut pending, &mut out);
                    out.push(Block::Rule);
                }
                Event::Start(tag) if is_block(&tag) => {
                    flush(&mut pending, &mut out);
                    self.block(tag, &mut out);
                }
                Event::Html(h) if pending.is_empty() => out.push(Block::Html(h.to_string())),
                other => {
                    if let Some(i) = self.inline(other) {
                        pending.push(i);
                    }
                }
            }
        }
        flush(&mut pending, &mut out);
        out
    }

    fn block(&mut self, tag: Tag<'a>, out: &mut Vec<Block>) {
        match tag {
            Tag::Paragraph => {
                let inlines = self.inlines();
                split_display_math(inlines, out);
            }
            Tag::Heading { level, id, .. } => {
                let content = self.inlines();
                let base = match id {
                    Some(id) => id.to_string(),
                    None => slugify(&plain_text(&content)),
                };
                let id = self.unique_slug(base);
                out.push(Block::Heading {
                    level: level as u8,
                    id,
                    content,
                });
            }
            Tag::BlockQuote(kind) => {
                let blocks = self.blocks();
                out.push(Block::BlockQuote {
                    kind: kind.map(admonition),
                    blocks,
                });
            }
            Tag::CodeBlock(kind) => {
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split(|c: char| c.is_whitespace() || c == ',' || c == '{')
                        .next()
                        .filter(|s| !s.is_empty())
                        .map(str::to_string),
                    CodeBlockKind::Indented => None,
                };
                let code = self.raw_text();
                out.push(Block::CodeBlock { lang, code });
            }
            Tag::HtmlBlock => {
                let html = self.raw_text();
                out.push(Block::Html(html));
            }
            Tag::List(start) => {
                let mut items = Vec::new();
                while let Some(ev) = self.next() {
                    match ev {
                        Event::Start(Tag::Item) => items.push(self.item()),
                        Event::End(_) => break,
                        _ => {}
                    }
                }
                let tight = items
                    .iter()
                    .all(|i| !i.blocks.iter().any(|b| matches!(b, Block::Paragraph(_))));
                out.push(Block::List {
                    start,
                    tight,
                    items,
                });
            }
            Tag::Table(aligns) => out.push(self.table(aligns)),
            Tag::FootnoteDefinition(label) => {
                let blocks = self.blocks();
                out.push(Block::FootnoteDef {
                    label: label.to_string(),
                    blocks,
                });
            }
            Tag::DefinitionList => {
                let mut entries: Vec<(Vec<Inline>, Vec<Vec<Block>>)> = Vec::new();
                while let Some(ev) = self.next() {
                    match ev {
                        Event::Start(Tag::DefinitionListTitle) => {
                            entries.push((self.inlines(), Vec::new()))
                        }
                        Event::Start(Tag::DefinitionListDefinition) => {
                            let defs = self.blocks();
                            match entries.last_mut() {
                                Some(e) => e.1.push(defs),
                                None => entries.push((Vec::new(), vec![defs])),
                            }
                        }
                        Event::End(_) => break,
                        _ => {}
                    }
                }
                out.push(Block::DefinitionList(entries));
            }
            Tag::MetadataBlock(_) => {
                let text = self.raw_text();
                self.front_matter = Some(text.trim_end().to_string());
            }
            // Item outside a list or unknown containers: keep the content.
            _ => out.extend(self.blocks()),
        }
    }

    fn item(&mut self) -> ListItem {
        let mut task = None;
        if let Some(Event::TaskListMarker(done)) = self.peek() {
            task = Some(*done);
            self.pos += 1;
        } else if matches!(self.peek(), Some(Event::Start(Tag::Paragraph)))
            && let Some(Event::TaskListMarker(done)) = self.events.get(self.pos + 1)
        {
            task = Some(*done);
            self.events.remove(self.pos + 1);
        }
        ListItem {
            task,
            blocks: self.blocks(),
        }
    }

    fn table(&mut self, aligns: Vec<Alignment>) -> Block {
        let aligns = aligns
            .into_iter()
            .map(|a| match a {
                Alignment::None => Align::None,
                Alignment::Left => Align::Left,
                Alignment::Center => Align::Center,
                Alignment::Right => Align::Right,
            })
            .collect();
        let mut header = Vec::new();
        let mut rows = Vec::new();
        while let Some(ev) = self.next() {
            match ev {
                Event::Start(Tag::TableHead) => header = self.cells(),
                Event::Start(Tag::TableRow) => rows.push(self.cells()),
                Event::End(TagEnd::Table) => break,
                _ => {}
            }
        }
        Block::Table {
            aligns,
            header,
            rows,
        }
    }

    fn cells(&mut self) -> Vec<Vec<Inline>> {
        let mut cells = Vec::new();
        while let Some(ev) = self.next() {
            match ev {
                Event::Start(Tag::TableCell) => cells.push(self.inlines()),
                Event::End(_) => break,
                _ => {}
            }
        }
        cells
    }

    /// Reads inline content until the enclosing End event (consumed).
    fn inlines(&mut self) -> Vec<Inline> {
        let mut out = Vec::new();
        while let Some(ev) = self.next() {
            if matches!(ev, Event::End(_)) {
                break;
            }
            if let Some(i) = self.inline(ev) {
                push_merged(&mut out, i);
            }
        }
        out
    }

    fn inline(&mut self, ev: Event<'a>) -> Option<Inline> {
        Some(match ev {
            Event::Text(t) => Inline::Text(t.to_string()),
            Event::Code(t) => Inline::Code(t.to_string()),
            Event::InlineMath(t) => Inline::Math(t.to_string()),
            Event::DisplayMath(t) => Inline::DisplayMath(t.to_string()),
            Event::Html(h) | Event::InlineHtml(h) => {
                let tag = h.trim().to_ascii_lowercase();
                if tag.starts_with("<br") {
                    Inline::HardBreak
                } else {
                    Inline::Html(h.to_string())
                }
            }
            Event::FootnoteReference(l) => Inline::FootnoteRef(l.to_string()),
            Event::SoftBreak => Inline::SoftBreak,
            Event::HardBreak => Inline::HardBreak,
            Event::Start(Tag::Emphasis) => Inline::Emph(self.inlines()),
            Event::Start(Tag::Strong) => Inline::Strong(self.inlines()),
            Event::Start(Tag::Strikethrough) => Inline::Strike(self.inlines()),
            Event::Start(Tag::Superscript) => Inline::Sup(self.inlines()),
            Event::Start(Tag::Subscript) => Inline::Sub(self.inlines()),
            Event::Start(Tag::Link { dest_url, .. }) => Inline::Link {
                url: dest_url.to_string(),
                content: self.inlines(),
            },
            Event::Start(Tag::Image { dest_url, .. }) => Inline::Image {
                url: dest_url.to_string(),
                alt: plain_text(&self.inlines()),
            },
            Event::Start(_) => {
                // Unknown inline container: keep its text.
                let inner = self.inlines();
                return (!inner.is_empty()).then(|| Inline::Text(plain_text(&inner)));
            }
            _ => return None,
        })
    }

    /// Concatenates text/html events until the enclosing End event.
    fn raw_text(&mut self) -> String {
        let mut s = String::new();
        while let Some(ev) = self.next() {
            match ev {
                Event::End(_) => break,
                Event::Text(t) | Event::Html(t) | Event::Code(t) => s.push_str(&t),
                _ => {}
            }
        }
        s
    }

    fn unique_slug(&mut self, base: String) -> String {
        let n = self.slugs.entry(base.clone()).or_insert(0);
        let slug = if *n == 0 {
            base.clone()
        } else {
            format!("{base}-{n}")
        };
        *n += 1;
        slug
    }
}

fn is_block(tag: &Tag) -> bool {
    matches!(
        tag,
        Tag::Paragraph
            | Tag::Heading { .. }
            | Tag::BlockQuote(_)
            | Tag::CodeBlock(_)
            | Tag::HtmlBlock
            | Tag::List(_)
            | Tag::Item
            | Tag::FootnoteDefinition(_)
            | Tag::DefinitionList
            | Tag::Table(_)
            | Tag::MetadataBlock(_)
    )
}

fn admonition(k: BlockQuoteKind) -> Admonition {
    match k {
        BlockQuoteKind::Note => Admonition::Note,
        BlockQuoteKind::Tip => Admonition::Tip,
        BlockQuoteKind::Important => Admonition::Important,
        BlockQuoteKind::Warning => Admonition::Warning,
        BlockQuoteKind::Caution => Admonition::Caution,
    }
}

fn flush(pending: &mut Vec<Inline>, out: &mut Vec<Block>) {
    if pending.is_empty() {
        return;
    }
    let inlines = std::mem::take(pending);
    if inlines
        .iter()
        .all(|i| matches!(i, Inline::Text(t) if t.trim().is_empty()) || matches!(i, Inline::SoftBreak))
    {
        return;
    }
    out.push(Block::Plain(inlines));
}

fn push_merged(out: &mut Vec<Inline>, i: Inline) {
    if let (Some(Inline::Text(prev)), Inline::Text(t)) = (out.last_mut(), &i) {
        prev.push_str(t);
    } else {
        out.push(i);
    }
}

/// Splits `$$...$$` out of a paragraph into standalone math blocks.
fn split_display_math(inlines: Vec<Inline>, out: &mut Vec<Block>) {
    let mut cur = Vec::new();
    for i in inlines {
        match i {
            Inline::DisplayMath(m) => {
                if has_content(&cur) {
                    out.push(Block::Paragraph(std::mem::take(&mut cur)));
                }
                cur.clear();
                out.push(Block::Math(m.trim().to_string()));
            }
            other => cur.push(other),
        }
    }
    if has_content(&cur) {
        while matches!(cur.first(), Some(Inline::SoftBreak | Inline::HardBreak)) {
            cur.remove(0);
        }
        out.push(Block::Paragraph(cur));
    }
}

fn has_content(v: &[Inline]) -> bool {
    v.iter().any(|i| match i {
        Inline::Text(t) => !t.trim().is_empty(),
        Inline::SoftBreak | Inline::HardBreak => false,
        _ => true,
    })
}

/// GitHub-style heading anchor.
pub fn slugify(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                Some(c)
            } else if c.is_whitespace() {
                Some('-')
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_slugs_are_unique() {
        let doc = parse("# Hello World!\n\n# Hello World!\n");
        let ids: Vec<_> = doc
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::Heading { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, ["hello-world", "hello-world-1"]);
    }

    #[test]
    fn task_list_markers() {
        let doc = parse("- [ ] todo\n- [x] done\n- plain\n");
        let Block::List { items, tight, .. } = &doc.blocks[0] else {
            panic!("expected list");
        };
        assert!(tight);
        let tasks: Vec<_> = items.iter().map(|i| i.task).collect();
        assert_eq!(tasks, [Some(false), Some(true), None]);
    }

    #[test]
    fn loose_task_list() {
        let doc = parse("- [x] one\n\n- [ ] two\n");
        let Block::List { items, tight, .. } = &doc.blocks[0] else {
            panic!("expected list");
        };
        assert!(!tight);
        assert_eq!(items[0].task, Some(true));
        assert_eq!(items[1].task, Some(false));
    }

    #[test]
    fn front_matter_and_admonition() {
        let doc = parse("---\ntitle: x\n---\n\n> [!WARNING]\n> careful\n");
        assert_eq!(doc.front_matter.as_deref(), Some("title: x"));
        assert!(matches!(
            doc.blocks[0],
            Block::BlockQuote {
                kind: Some(Admonition::Warning),
                ..
            }
        ));
    }

    #[test]
    fn table_and_code() {
        let doc = parse("| a | b |\n|:--|--:|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n");
        let Block::Table { aligns, header, rows } = &doc.blocks[0] else {
            panic!("expected table");
        };
        assert_eq!(aligns, &[Align::Left, Align::Right]);
        assert_eq!(header.len(), 2);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            doc.blocks[1],
            Block::CodeBlock {
                lang: Some("rust".into()),
                code: "fn main() {}\n".into()
            }
        );
    }

    #[test]
    fn display_math_becomes_block() {
        let doc = parse("$$\nx^2\n$$\n");
        assert_eq!(doc.blocks, [Block::Math("x^2".into())]);
    }
}
