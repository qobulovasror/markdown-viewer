//! Color themes for rendered documents.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
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
#[derive(Clone)]
struct Palette {
    name: &'static str,
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
    /// Use `fg` for body text instead of the terminal's own color.
    own_fg: bool,
}

impl Palette {
    fn build(&self) -> Theme {
        let b = Modifier::BOLD;
        // Default themes keep the terminal's own foreground for body text.
        let text = if !self.own_fg {
            Style::new()
        } else {
            fg(self.fg)
        };
        Theme {
            name: self.name,
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
        palette(name).map(|p| p.build())
    }

    /// A built-in theme or one from `themes/<name>.toml` in the config folder.
    pub fn load(name: &str) -> Result<Theme> {
        if let Some(t) = Theme::by_name(name) {
            return Ok(t);
        }
        if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
            bail!("invalid theme name '{name}'");
        }
        let path = themes_dir()
            .map(|d| d.join(format!("{name}.toml")))
            .filter(|p| p.is_file());
        match path {
            Some(p) => load_file(name, &p),
            None => bail!(
                "unknown theme '{name}' (available: {})",
                all_names().join(", ")
            ),
        }
    }

    pub fn default_dark() -> Theme {
        Theme::by_name("dark").expect("built-in theme")
    }
}

/// Built-in theme names followed by custom ones.
pub fn all_names() -> Vec<String> {
    let mut names: Vec<String> = THEME_NAMES.iter().map(|s| s.to_string()).collect();
    names.extend(custom_names());
    names
}

fn themes_dir() -> Option<PathBuf> {
    crate::config::config_dir().map(|d| d.join("themes"))
}

/// Names of `*.toml` files in the themes folder, sorted.
pub fn custom_names() -> Vec<String> {
    let Some(dir) = themes_dir() else {
        return Vec::new();
    };
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()? == "toml").then(|| p.file_stem()?.to_str().map(str::to_string))?
        })
        .filter(|n| !THEME_NAMES.contains(&n.as_str()))
        .collect();
    names.sort();
    names
}

/// A theme file: a built-in base plus color overrides (`#rrggbb`).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeFile {
    base: Option<String>,
    /// syntect theme for code blocks, e.g. "base16-ocean.dark".
    syntax: Option<String>,
    #[serde(default)]
    colors: ThemeColors,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ThemeColors {
    fg: Option<String>,
    muted: Option<String>,
    surface: Option<String>,
    /// Heading colors, levels 1-6.
    accents: Option<Vec<String>>,
    code: Option<String>,
    link: Option<String>,
    green: Option<String>,
    yellow: Option<String>,
    red: Option<String>,
    blue: Option<String>,
    purple: Option<String>,
}

fn load_file(name: &str, path: &Path) -> Result<Theme> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse_theme(name, &text).with_context(|| format!("invalid theme {}", path.display()))
}

fn parse_theme(name: &str, text: &str) -> Result<Theme> {
    let file: ThemeFile = toml::from_str(text)?;
    let base = file.base.as_deref().unwrap_or("dark");
    let mut p = palette(base).with_context(|| {
        format!(
            "unknown base '{base}' (use one of {})",
            THEME_NAMES.join(", ")
        )
    })?;
    p.name = intern(name);
    let c = &file.colors;
    let set = |slot: &mut u32, v: &Option<String>| -> Result<()> {
        if let Some(v) = v {
            *slot = hex(v)?;
        }
        Ok(())
    };
    if c.fg.is_some() {
        p.own_fg = true;
    }
    set(&mut p.fg, &c.fg)?;
    set(&mut p.muted, &c.muted)?;
    set(&mut p.surface, &c.surface)?;
    set(&mut p.code, &c.code)?;
    set(&mut p.link, &c.link)?;
    set(&mut p.green, &c.green)?;
    set(&mut p.yellow, &c.yellow)?;
    set(&mut p.red, &c.red)?;
    set(&mut p.blue, &c.blue)?;
    set(&mut p.purple, &c.purple)?;
    if let Some(acc) = &c.accents {
        if acc.is_empty() || acc.len() > 6 {
            bail!("accents needs 1 to 6 colors");
        }
        for (i, slot) in p.accents.iter_mut().enumerate() {
            // Fewer than six repeat the last one.
            *slot = hex(&acc[i.min(acc.len() - 1)])?;
        }
    }
    if let Some(s) = file.syntax {
        if !crate::highlight::has_theme(&s) {
            bail!(
                "unknown syntax theme '{s}' (available: {})",
                crate::highlight::theme_names().join(", ")
            );
        }
        p.syntax = intern(&s);
    }
    Ok(p.build())
}

/// A `'static` copy of `s`, made once per distinct string (theme names live
/// for the whole run and `T` reloads custom themes).
fn intern(s: &str) -> &'static str {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};
    static SET: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut set = SET
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(&v) = set.get(s) {
        return v;
    }
    let v: &'static str = Box::leak(s.to_string().into_boxed_str());
    set.insert(v);
    v
}

/// `#rrggbb` or `rrggbb`.
fn hex(s: &str) -> Result<u32> {
    let t = s.trim();
    let h = t.strip_prefix('#').unwrap_or(t);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        bail!("bad color '{s}' (expected #rrggbb)");
    }
    u32::from_str_radix(h, 16).with_context(|| format!("bad color '{s}'"))
}

fn palette(name: &str) -> Option<Palette> {
    let p = match name {
        "dark" => Palette {
            name: "dark",
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
            own_fg: false,
        },
        "light" => Palette {
            name: "light",
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
            own_fg: false,
        },
        "dracula" => Palette {
            name: "dracula",
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
            own_fg: true,
        },
        "nord" => Palette {
            name: "nord",
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
            own_fg: true,
        },
        "gruvbox" => Palette {
            name: "gruvbox",
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
            own_fg: true,
        },
        _ => return None,
    };
    Some(p)
}

/// Picks `light` or `dark` from the terminal background (OSC 11 / COLORFGBG).
pub fn auto() -> Theme {
    let light = terminal_light::luma().is_ok_and(|l| l > 0.6);
    Theme::by_name(if light { "light" } else { "dark" }).expect("built-in theme")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_theme_overrides_base() {
        let t = parse_theme(
            "mine",
            "base = \"nord\"\nsyntax = \"InspiredGitHub\"\n[colors]\nlink = \"#ff0000\"\naccents = [\"#010203\"]\n",
        )
        .unwrap();
        assert_eq!(t.name, "mine");
        assert_eq!(t.syntax, "InspiredGitHub");
        assert_eq!(t.link.fg, Some(Color::Rgb(255, 0, 0)));
        assert_eq!(t.headings[5].fg, Some(Color::Rgb(1, 2, 3)));
        // nord's text color is kept.
        assert_eq!(t.text, Theme::by_name("nord").unwrap().text);
    }

    #[test]
    fn custom_theme_errors() {
        assert!(parse_theme("x", "base = \"nope\"").is_err());
        assert!(parse_theme("x", "[colors]\nfg = \"#12\"").is_err());
        assert!(parse_theme("x", "[colors]\nfg = \"+12345\"").is_err());
        assert!(parse_theme("x", "colour = 1").is_err());
        assert!(parse_theme("x", "syntax = \"nope\"").is_err());
        let t = parse_theme("x", "[colors]\nfg = \"#101010\"").unwrap();
        assert_eq!(t.text.fg, Some(Color::Rgb(16, 16, 16)));
    }
}
