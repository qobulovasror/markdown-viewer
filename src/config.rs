//! Optional user configuration (`~/.config/mdv/config.toml`).

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Theme name; unset means auto light/dark.
    pub theme: Option<String>,
    /// Maximum text width in the viewer (0 = full terminal width).
    pub max_width: usize,
    pub mouse: bool,
    pub watch: bool,
    pub hyperlinks: bool,
    /// Open the table of contents on start.
    pub toc: bool,
    /// Remember the reading position per file.
    pub remember_position: bool,
    /// Show images when the terminal supports Kitty, iTerm2 or Sixel graphics.
    pub images: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: None,
            max_width: 100,
            mouse: true,
            watch: false,
            hyperlinks: true,
            toc: false,
            remember_position: true,
            images: true,
        }
    }
}

/// `$XDG_CONFIG_HOME/mdv` or `~/.config/mdv` (APPDATA on Windows).
pub fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return dirs::config_dir().map(|d| d.join("mdv"));
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .map(|d| d.join("mdv"))
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        config_dir().map(|d| d.join("config.toml"))
    }

    /// Loads the config file; a missing file yields defaults.
    pub fn load() -> Result<Config> {
        let Some(path) = Config::path() else {
            return Ok(Config::default());
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_partial_config() {
        let c: Config = toml::from_str("theme = \"nord\"\nmax_width = 80\n").unwrap();
        assert_eq!(c.theme.as_deref(), Some("nord"));
        assert_eq!(c.max_width, 80);
        assert!(c.mouse);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(toml::from_str::<Config>("colour = 1").is_err());
    }
}
