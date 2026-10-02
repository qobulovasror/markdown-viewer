//! Plain text: paragraphs split on blank lines, line breaks preserved.

use super::ast::*;

pub fn parse(src: &str) -> Document {
    let mut blocks = Vec::new();
    for para in src
        .split("\n\n")
        .map(|p| p.trim_matches('\n'))
        .filter(|p| !p.trim().is_empty())
    {
        let mut inl = Vec::new();
        for (i, line) in para.lines().enumerate() {
            if i > 0 {
                inl.push(Inline::HardBreak);
            }
            inl.push(Inline::Text(line.trim_end().replace('\t', "    ")));
        }
        blocks.push(Block::Paragraph(inl));
    }
    Document {
        front_matter: None,
        blocks,
    }
}
