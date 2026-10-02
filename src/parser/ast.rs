//! Format-independent document model shared by all parsers.

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Document {
    pub front_matter: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admonition {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading {
        level: u8,
        id: String,
        content: Vec<Inline>,
    },
    /// Loose paragraph, separated from neighbours by a blank line.
    Paragraph(Vec<Inline>),
    /// Tight text (e.g. inside a tight list item), no blank line after it.
    Plain(Vec<Inline>),
    CodeBlock {
        lang: Option<String>,
        code: String,
    },
    BlockQuote {
        kind: Option<Admonition>,
        blocks: Vec<Block>,
    },
    List {
        start: Option<u64>,
        tight: bool,
        items: Vec<ListItem>,
    },
    Table {
        aligns: Vec<Align>,
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    DefinitionList(Vec<(Vec<Inline>, Vec<Vec<Block>>)>),
    FootnoteDef {
        label: String,
        blocks: Vec<Block>,
    },
    Math(String),
    Html(String),
    Rule,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ListItem {
    pub task: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Emph(Vec<Inline>),
    Strong(Vec<Inline>),
    Strike(Vec<Inline>),
    Sup(Vec<Inline>),
    Sub(Vec<Inline>),
    Link {
        url: String,
        content: Vec<Inline>,
    },
    Image {
        url: String,
        alt: String,
    },
    FootnoteRef(String),
    Math(String),
    /// `$$...$$`; the parser lifts these into `Block::Math`.
    DisplayMath(String),
    Html(String),
    SoftBreak,
    HardBreak,
}

/// Concatenated plain text of inline content (used for slugs, TOC, alt text).
pub fn plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    collect_text(inlines, &mut out);
    out
}

fn collect_text(inlines: &[Inline], out: &mut String) {
    for i in inlines {
        match i {
            Inline::Text(t) | Inline::Code(t) | Inline::Math(t) | Inline::DisplayMath(t) => out.push_str(t),
            Inline::Emph(c)
            | Inline::Strong(c)
            | Inline::Strike(c)
            | Inline::Sup(c)
            | Inline::Sub(c)
            | Inline::Link { content: c, .. } => collect_text(c, out),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::FootnoteRef(l) => {
                out.push('[');
                out.push_str(l);
                out.push(']');
            }
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Html(_) => {}
        }
    }
}
