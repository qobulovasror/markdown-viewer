//! Renders laid-out lines as ANSI-escaped text for stdout.

use std::fmt::Write;

use ratatui::style::{Color, Modifier, Style};

use crate::color::ColorDepth;
use crate::layout::Rendered;

pub struct AnsiOptions {
    pub depth: ColorDepth,
    /// Emit OSC 8 hyperlinks for absolute URLs.
    pub hyperlinks: bool,
}

pub fn render(doc: &Rendered, opts: &AnsiOptions) -> String {
    let mut out = String::new();
    let plain = opts.depth == ColorDepth::None;
    for line in &doc.lines {
        for span in &line.spans {
            let url = span
                .link
                .map(|i| doc.links[i].as_str())
                .filter(|u| opts.hyperlinks && !plain && is_external(u));
            if let Some(u) = url {
                let _ = write!(out, "\x1b]8;;{u}\x1b\\");
            }
            if plain {
                out.push_str(&span.text);
            } else {
                let sgr = sgr(opts.depth.adapt(span.style));
                if sgr.is_empty() {
                    out.push_str(&span.text);
                } else {
                    let _ = write!(out, "\x1b[{sgr}m{}\x1b[0m", span.text);
                }
            }
            if url.is_some() {
                out.push_str("\x1b]8;;\x1b\\");
            }
        }
        // Drop trailing padding so piped output stays clean.
        let trimmed = out.trim_end_matches(' ').len();
        out.truncate(trimmed);
        out.push('\n');
    }
    out
}

pub fn is_external(url: &str) -> bool {
    url.contains("://") || url.starts_with("mailto:")
}

fn sgr(style: Style) -> String {
    let mut codes: Vec<String> = Vec::new();
    let m = style.add_modifier;
    for (flag, code) in [
        (Modifier::BOLD, "1"),
        (Modifier::DIM, "2"),
        (Modifier::ITALIC, "3"),
        (Modifier::UNDERLINED, "4"),
        (Modifier::REVERSED, "7"),
        (Modifier::CROSSED_OUT, "9"),
    ] {
        if m.contains(flag) {
            codes.push(code.into());
        }
    }
    if let Some(c) = style.fg {
        codes.push(color_code(c, false));
    }
    if let Some(c) = style.bg {
        codes.push(color_code(c, true));
    }
    codes.retain(|c| !c.is_empty());
    codes.join(";")
}

fn color_code(c: Color, bg: bool) -> String {
    let base = if bg { 40 } else { 30 };
    let named = |n: u8, bright: bool| {
        let n = n as u16 + base + if bright { 60 } else { 0 };
        n.to_string()
    };
    match c {
        Color::Reset => String::new(),
        Color::Black => named(0, false),
        Color::Red => named(1, false),
        Color::Green => named(2, false),
        Color::Yellow => named(3, false),
        Color::Blue => named(4, false),
        Color::Magenta => named(5, false),
        Color::Cyan => named(6, false),
        Color::Gray => named(7, false),
        Color::DarkGray => named(0, true),
        Color::LightRed => named(1, true),
        Color::LightGreen => named(2, true),
        Color::LightYellow => named(3, true),
        Color::LightBlue => named(4, true),
        Color::LightMagenta => named(5, true),
        Color::LightCyan => named(6, true),
        Color::White => named(7, true),
        Color::Indexed(i) => format!("{};5;{i}", base + 8),
        Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", base + 8),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sgr_codes() {
        let s = Style::new().fg(Color::Rgb(1, 2, 3)).add_modifier(Modifier::BOLD);
        assert_eq!(sgr(s), "1;38;2;1;2;3");
        assert_eq!(color_code(Color::LightRed, true), "101");
    }
}
