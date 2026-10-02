//! Color themes for rendered documents.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub dark: bool,
    pub text: Style,
    pub emph: Style,
    pub strong: Style,
    pub strike: Style,
    pub code: Style,
    pub link: Style,
    pub image: Style,
    pub footnote: Style,
    pub math: Style,
    pub dim: Style,
    pub headings: [Style; 6],
    pub code_border: Style,
    pub code_lang: Style,
    pub code_text: Style,
    pub quote_bar: Style,
    pub quote_text: Style,
    pub admonitions: [Style; 5],
    pub bullet: Style,
    pub task_done: Style,
    pub task_todo: Style,
    pub table_border: Style,
    pub table_header: Style,
    pub rule: Style,
    /// Background used for the UI chrome (status bar, panels).
    pub ui_bar: Style,
    pub ui_accent: Style,
    pub search_match: Style,
    pub search_current: Style,
    pub link_focus: Style,
    /// syntect theme used for code blocks.
    pub syntax: &'static str,
}

const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn fg(hex: u32) -> Style {
    Style::new().fg(rgb(hex))
}

/// Palette from which a full theme is derived.
struct Palette {
    name: &'static str,
    dark: bool,
    fg: u32,
    muted: u32,
    surface: u32,
    accents: [u32; 6],
    code: u32,
    link: u32,
    green: u32,
    yellow: u32,
    red: u32,
    blue: u32,
    purple: u32,
    syntax: &'static str,
}

impl Palette {
    fn build(&self) -> Theme {
        let b = Modifier::BOLD;
        // Default themes keep the terminal's own foreground for body text.
        let text = if matches!(self.name, "dark" | "light") {
            Style::new()
        } else {
            fg(self.fg)
        };
        Theme {
            name: self.name,
            dark: self.dark,
            text,
            emph: Style::new().add_modifier(Modifier::ITALIC),
            strong: Style::new().add_modifier(b),
            strike: Style::new().add_modifier(Modifier::CROSSED_OUT),
            code: fg(self.code).bg(rgb(self.surface)),
            link: fg(self.link).add_modifier(Modifier::UNDERLINED),
            image: fg(self.purple),
            footnote: fg(self.link),
            math: fg(self.yellow).add_modifier(Modifier::ITALIC),
            dim: fg(self.muted),
            headings: self.accents.map(|c| fg(c).add_modifier(b)),
            code_border: fg(self.muted),
            code_lang: fg(self.accents[1]).add_modifier(b),
            code_text: text,
            quote_bar: fg(self.muted),
            quote_text: fg(self.muted).add_modifier(Modifier::ITALIC),
            admonitions: [
                fg(self.blue),
                fg(self.green),
                fg(self.purple),
                fg(self.yellow),
                fg(self.red),
            ]
            .map(|s| s.add_modifier(b)),
            bullet: fg(self.accents[1]),
            task_done: fg(self.green),
            task_todo: fg(self.muted),
            table_border: fg(self.muted),
            table_header: fg(self.accents[0]).add_modifier(b),
            rule: fg(self.muted),
            ui_bar: fg(self.fg).bg(rgb(self.surface)),
            ui_accent: fg(self.accents[0]).add_modifier(b),
            search_match: Style::new().fg(rgb(0x000000)).bg(rgb(self.yellow)),
            search_current: Style::new()
                .fg(rgb(0x000000))
                .bg(rgb(self.red))
                .add_modifier(b),
            link_focus: Style::new().add_modifier(Modifier::REVERSED),
            syntax: self.syntax,
        }
    }
}

pub const THEME_NAMES: &[&str] = &["dark", "light", "dracula", "nord", "gruvbox"];

impl Theme {
    pub fn by_name(name: &str) -> Option<Theme> {
        let p = match name {
            "dark" => Palette {
                name: "dark",
                dark: true,
                fg: 0xd4d4d4,
                muted: 0x7a7f8a,
                surface: 0x2a2d35,
                accents: [0x61afef, 0xc678dd, 0x56b6c2, 0xe5c07b, 0x98c379, 0xabb2bf],
                code: 0xe5c07b,
                link: 0x61afef,
                green: 0x98c379,
                yellow: 0xe5c07b,
                red: 0xe06c75,
                blue: 0x61afef,
                purple: 0xc678dd,
                syntax: "base16-ocean.dark",
            },
            "light" => Palette {
                name: "light",
                dark: false,
                fg: 0x24292f,
                muted: 0x6e7781,
                surface: 0xeaeef2,
                accents: [0x0550ae, 0x8250df, 0x1b7c83, 0x953800, 0x116329, 0x57606a],
                code: 0x953800,
                link: 0x0969da,
                green: 0x1a7f37,
                yellow: 0x9a6700,
                red: 0xcf222e,
                blue: 0x0969da,
                purple: 0x8250df,
                syntax: "InspiredGitHub",
            },
            "dracula" => Palette {
                name: "dracula",
                dark: true,
                fg: 0xf8f8f2,
                muted: 0x6272a4,
                surface: 0x343746,
                accents: [0xbd93f9, 0xff79c6, 0x8be9fd, 0xf1fa8c, 0x50fa7b, 0xffb86c],
                code: 0x50fa7b,
                link: 0x8be9fd,
                green: 0x50fa7b,
                yellow: 0xf1fa8c,
                red: 0xff5555,
                blue: 0x8be9fd,
                purple: 0xbd93f9,
                syntax: "base16-eighties.dark",
            },
            "nord" => Palette {
                name: "nord",
                dark: true,
                fg: 0xd8dee9,
                muted: 0x616e88,
                surface: 0x3b4252,
                accents: [0x88c0d0, 0x81a1c1, 0x8fbcbb, 0xebcb8b, 0xa3be8c, 0xb48ead],
                code: 0xebcb8b,
                link: 0x88c0d0,
                green: 0xa3be8c,
                yellow: 0xebcb8b,
                red: 0xbf616a,
                blue: 0x81a1c1,
                purple: 0xb48ead,
                syntax: "base16-ocean.dark",
            },
            "gruvbox" => Palette {
                name: "gruvbox",
                dark: true,
                fg: 0xebdbb2,
                muted: 0x928374,
                surface: 0x3c3836,
                accents: [0xfabd2f, 0xfe8019, 0x8ec07c, 0x83a598, 0xb8bb26, 0xd3869b],
                code: 0xfe8019,
                link: 0x83a598,
                green: 0xb8bb26,
                yellow: 0xfabd2f,
                red: 0xfb4934,
                blue: 0x83a598,
                purple: 0xd3869b,
                syntax: "base16-mocha.dark",
            },
            _ => return None,
        };
        Some(p.build())
    }

    pub fn default_dark() -> Theme {
        Theme::by_name("dark").expect("built-in theme")
    }
}
