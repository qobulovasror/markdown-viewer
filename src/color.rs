//! Terminal color capability detection and color downgrading.

use ratatui::style::{Color, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Ansi256,
    Ansi16,
    None,
}

impl ColorDepth {
    pub fn detect() -> ColorDepth {
        if std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
            return ColorDepth::None;
        }
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        if colorterm.contains("truecolor") || colorterm.contains("24bit") {
            return ColorDepth::TrueColor;
        }
        let term = std::env::var("TERM").unwrap_or_default();
        let program = std::env::var("TERM_PROGRAM").unwrap_or_default();
        if matches!(
            program.as_str(),
            "iTerm.app" | "WezTerm" | "vscode" | "ghostty" | "Hyper"
        ) || term.contains("kitty")
            || term.contains("alacritty")
            || term.contains("direct")
        {
            ColorDepth::TrueColor
        } else if term.contains("256") || program == "Apple_Terminal" {
            ColorDepth::Ansi256
        } else if term.is_empty() || term == "dumb" {
            ColorDepth::None
        } else {
            ColorDepth::Ansi16
        }
    }

    pub fn adapt(self, style: Style) -> Style {
        match self {
            ColorDepth::TrueColor => style,
            ColorDepth::None => Style {
                fg: None,
                bg: None,
                underline_color: None,
                ..style
            },
            _ => Style {
                fg: style.fg.map(|c| self.color(c)),
                bg: style.bg.map(|c| self.color(c)),
                underline_color: style.underline_color.map(|c| self.color(c)),
                ..style
            },
        }
    }

    fn color(self, c: Color) -> Color {
        let Color::Rgb(r, g, b) = c else {
            return c;
        };
        match self {
            ColorDepth::Ansi256 => Color::Indexed(rgb_to_256(r, g, b)),
            ColorDepth::Ansi16 => rgb_to_16(r, g, b),
            _ => c,
        }
    }
}

fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    let gray = r.abs_diff(g) < 10 && g.abs_diff(b) < 10;
    if gray {
        let avg = (r as u16 + g as u16 + b as u16) / 3;
        if avg < 8 {
            return 16;
        }
        if avg > 246 {
            return 231;
        }
        return 232 + ((avg - 8) * 24 / 239) as u8;
    }
    let q = |v: u8| -> u8 {
        if v < 48 {
            0
        } else if v < 115 {
            1
        } else {
            (v - 35) / 40
        }
    };
    16 + 36 * q(r) + 6 * q(g) + q(b)
}

fn rgb_to_16(r: u8, g: u8, b: u8) -> Color {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let sat = if max == 0.0 { 0.0 } else { (max - min) / max };
    if sat < 0.25 {
        return match max {
            v if v < 0.25 => Color::Black,
            v if v < 0.55 => Color::DarkGray,
            v if v < 0.85 => Color::Gray,
            _ => Color::White,
        };
    }
    let d = max - min;
    let hue = if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let bright = max > 0.7;
    let pick = |dark: Color, light: Color| if bright { light } else { dark };
    match hue {
        h if !(20.0..330.0).contains(&h) => pick(Color::Red, Color::LightRed),
        h if h < 70.0 => pick(Color::Yellow, Color::LightYellow),
        h if h < 160.0 => pick(Color::Green, Color::LightGreen),
        h if h < 200.0 => pick(Color::Cyan, Color::LightCyan),
        h if h < 260.0 => pick(Color::Blue, Color::LightBlue),
        _ => pick(Color::Magenta, Color::LightMagenta),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cube_mapping() {
        assert_eq!(rgb_to_256(255, 0, 0), 196);
        assert_eq!(rgb_to_256(0, 0, 0), 16);
        assert_eq!(rgb_to_256(255, 255, 255), 231);
    }

    #[test]
    fn sixteen_mapping() {
        assert_eq!(rgb_to_16(0xe0, 0x6c, 0x75), Color::LightRed);
        assert_eq!(rgb_to_16(0x61, 0xaf, 0xef), Color::LightBlue);
    }
}
