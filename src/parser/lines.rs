//! Line-oriented helpers shared by the lightweight format parsers.

use std::collections::HashMap;

use super::ast::{Block, Inline, ListItem, plain_text};
use super::markdown::slugify;

pub fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches([' ', '\t']).len()
}

pub fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// Removes up to `n` leading spaces from each line.
pub fn dedent<'a>(lines: &[&'a str], n: usize) -> Vec<&'a str> {
    lines
        .iter()
        .map(|l| &l[indent(l).min(n)..])
        .collect()
}

/// Unique GitHub-style heading ids.
#[derive(Default)]
pub struct Slugger(HashMap<String, usize>);

impl Slugger {
    pub fn heading(&mut self, level: u8, content: Vec<Inline>) -> Block {
        let base = slugify(&plain_text(&content));
        let n = self.0.entry(base.clone()).or_insert(0);
        let id = if *n == 0 { base } else { format!("{base}-{n}") };
        *n += 1;
        Block::Heading { level, id, content }
    }
}

/// A list item marker found at the start of a line.
pub struct Marker {
    pub indent: usize,
    /// Byte column where the item text starts.
    pub content: usize,
    pub ordered: Option<u64>,
}

/// Collects an indentation-based list starting at `lines[0]`.
///
/// Returns the list block and the number of lines consumed.
pub fn collect_list(
    lines: &[&str],
    marker: impl Fn(&str) -> Option<Marker>,
    mut parse_blocks: impl FnMut(&[&str]) -> Vec<Block>,
) -> (Block, usize) {
    let first = marker(lines[0]).expect("caller checked marker");
    let ordered = first.ordered.is_some();
    let mut items = Vec::new();
    let mut loose = false;
    let mut i = 0;
    while i < lines.len() {
        let Some(m) = marker(lines[i]) else { break };
        if m.indent != first.indent || m.ordered.is_some() != ordered {
            break;
        }
        let mut body = vec![&lines[i][m.content..]];
        i += 1;
        while i < lines.len() && (is_blank(lines[i]) || indent(lines[i]) > first.indent) {
            body.push(lines[i]);
            i += 1;
        }
        // Trailing blank lines separate items rather than belong to them.
        let mut trailing = 0;
        while body.len() > 1 && is_blank(body[body.len() - 1]) {
            body.pop();
            trailing += 1;
        }
        if trailing > 0 && i < lines.len() && marker(lines[i]).is_some_and(|n| n.indent == first.indent) {
            loose = true;
        }
        let rest = dedent(&body[1..], m.content);
        let mut text = vec![body[0]];
        text.extend(rest);

        let (task, text) = task_marker(text);
        let mut blocks = parse_blocks(&text);
        if let Some(Block::Paragraph(inl)) = blocks.first().cloned() {
            blocks[0] = Block::Plain(inl);
        }
        items.push(ListItem { task, blocks });
    }
    if loose {
        for item in &mut items {
            if let Some(Block::Plain(inl)) = item.blocks.first().cloned() {
                item.blocks[0] = Block::Paragraph(inl);
            }
        }
    }
    let consumed = i;
    (
        Block::List {
            start: first.ordered,
            tight: !loose,
            items,
        },
        consumed,
    )
}

/// Strips a leading `[ ]` / `[x]` checkbox from the first line.
fn task_marker(mut text: Vec<&str>) -> (Option<bool>, Vec<&str>) {
    let first = text[0];
    for (prefix, done) in [("[ ] ", false), ("[x] ", true), ("[X] ", true), ("[-] ", false)] {
        if let Some(rest) = first.strip_prefix(prefix) {
            text[0] = rest;
            return (Some(done), text);
        }
    }
    (None, text)
}

/// Parses `1.`, `1)`, `-`, `+`, `*` style markers (bullets given by caller).
pub fn simple_marker(line: &str, bullets: &[char], allow_paren: bool) -> Option<Marker> {
    let ind = indent(line);
    let rest = &line[ind..];
    let mut chars = rest.char_indices();
    let (_, c) = chars.next()?;
    if bullets.contains(&c) {
        let after = &rest[1..];
        if after.starts_with(' ') || after.is_empty() {
            let content = ind + 1 + (after.len() - after.trim_start().len());
            return Some(Marker {
                indent: ind,
                content: content.min(line.len()),
                ordered: None,
            });
        }
        return None;
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let delim = rest[digits..].chars().next()?;
    if delim != '.' && !(allow_paren && delim == ')') {
        return None;
    }
    let after = &rest[digits + 1..];
    if !(after.starts_with(' ') || after.is_empty()) {
        return None;
    }
    Some(Marker {
        indent: ind,
        content: (ind + digits + 1 + (after.len() - after.trim_start().len())).min(line.len()),
        ordered: rest[..digits].parse().ok(),
    })
}

/// Splits a `| a | b |` row into trimmed cells.
pub fn table_cells(line: &str) -> Vec<&str> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(str::trim).collect()
}
