//! Single-line text input with a cursor and readline-style keys.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Default, Clone)]
pub struct LineInput {
    pub text: String,
    /// Byte offset of the cursor, always on a char boundary.
    cursor: usize,
}

/// What a key did to the input.
#[derive(Debug, PartialEq, Eq)]
pub enum Edit {
    Changed,
    Moved,
    Ignored,
}

impl LineInput {
    pub fn set(&mut self, text: &str) {
        self.text = text.to_string();
        self.cursor = self.text.len();
    }

    pub fn clear(&mut self) {
        self.set("");
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Text before and after the cursor.
    pub fn split(&self) -> (&str, &str) {
        self.text.split_at(self.cursor)
    }

    fn prev(&self) -> usize {
        self.text[..self.cursor]
            .char_indices()
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next(&self) -> usize {
        self.text[self.cursor..]
            .chars()
            .next()
            .map_or(self.cursor, |c| self.cursor + c.len_utf8())
    }

    /// Start of the word before the cursor (skipping trailing spaces).
    fn word_start(&self) -> usize {
        let before = &self.text[..self.cursor];
        let trimmed = before.trim_end();
        trimmed
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map_or(0, |(i, c)| i + c.len_utf8())
    }

    fn move_to(&mut self, pos: usize) -> Edit {
        self.cursor = pos;
        Edit::Moved
    }

    pub fn handle(&mut self, key: &KeyEvent) -> Edit {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Left => self.move_to(self.prev()),
            KeyCode::Char('b') if ctrl => self.move_to(self.prev()),
            KeyCode::Right => self.move_to(self.next()),
            KeyCode::Char('f') if ctrl => self.move_to(self.next()),
            KeyCode::Home => self.move_to(0),
            KeyCode::Char('a') if ctrl => self.move_to(0),
            KeyCode::End => self.move_to(self.text.len()),
            KeyCode::Char('e') if ctrl => self.move_to(self.text.len()),
            KeyCode::Backspace if self.cursor > 0 => {
                let p = self.prev();
                self.text.replace_range(p..self.cursor, "");
                self.cursor = p;
                Edit::Changed
            }
            KeyCode::Delete if self.cursor < self.text.len() => {
                let n = self.next();
                self.text.replace_range(self.cursor..n, "");
                Edit::Changed
            }
            KeyCode::Char('u') if ctrl => {
                self.text.replace_range(..self.cursor, "");
                self.cursor = 0;
                Edit::Changed
            }
            KeyCode::Char('k') if ctrl => {
                self.text.truncate(self.cursor);
                Edit::Changed
            }
            KeyCode::Char('w') if ctrl => {
                let s = self.word_start();
                self.text.replace_range(s..self.cursor, "");
                self.cursor = s;
                Edit::Changed
            }
            KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                self.text.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                Edit::Changed
            }
            _ => Edit::Ignored,
        }
    }
}

/// Recall list for an input (newest last), browsed with Up/Down.
#[derive(Debug, Default)]
pub struct History {
    items: Vec<String>,
    /// Position while browsing; `None` means the live input.
    pos: Option<usize>,
    draft: String,
}

impl History {
    pub fn push(&mut self, s: &str) {
        self.pos = None;
        if s.is_empty() {
            return;
        }
        self.items.retain(|x| x != s);
        self.items.push(s.to_string());
        if self.items.len() > 100 {
            self.items.remove(0);
        }
    }

    pub fn reset(&mut self) {
        self.pos = None;
    }

    /// Older (`back`) or newer entry; returns the text to show.
    pub fn step(&mut self, back: bool, current: &str) -> Option<String> {
        let n = self.items.len();
        let next = match (self.pos, back) {
            (None, true) if n > 0 => {
                self.draft = current.to_string();
                Some(n - 1)
            }
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i + 1 < n => Some(i + 1),
            (Some(_), false) => {
                self.pos = None;
                return Some(std::mem::take(&mut self.draft));
            }
            _ => return None,
        };
        self.pos = next;
        next.map(|i| self.items[i].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn editing() {
        let mut i = LineInput::default();
        for c in "héllo wörld".chars() {
            i.handle(&key(KeyCode::Char(c)));
        }
        i.handle(&ctrl('w'));
        assert_eq!(i.text, "héllo ");
        i.handle(&key(KeyCode::Left));
        i.handle(&key(KeyCode::Left));
        i.handle(&key(KeyCode::Char('X')));
        assert_eq!(i.split(), ("héllX", "o "));
        i.handle(&key(KeyCode::Backspace));
        i.handle(&key(KeyCode::Delete));
        assert_eq!(i.text, "héll ");
        i.handle(&ctrl('k'));
        assert_eq!(i.text, "héll");
        i.handle(&ctrl('a'));
        i.handle(&key(KeyCode::Right));
        i.handle(&ctrl('u'));
        assert_eq!(i.text, "éll");
        assert_eq!(i.handle(&key(KeyCode::Backspace)), Edit::Ignored);
        assert_eq!(i.handle(&ctrl('b')), Edit::Moved);
        assert_eq!(i.handle(&key(KeyCode::Char('b'))), Edit::Changed);
    }

    #[test]
    fn history_browsing() {
        let mut h = History::default();
        assert_eq!(h.step(true, "x"), None);
        h.push("one");
        h.push("two");
        h.push("one");
        assert_eq!(h.step(true, "dr").as_deref(), Some("one"));
        assert_eq!(h.step(true, "").as_deref(), Some("two"));
        assert_eq!(h.step(true, "").as_deref(), Some("two"));
        assert_eq!(h.step(false, "").as_deref(), Some("one"));
        assert_eq!(h.step(false, "").as_deref(), Some("dr"));
        assert_eq!(h.step(false, ""), None);
    }
}
