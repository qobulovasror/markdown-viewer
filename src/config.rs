//! Optional user configuration (`~/.config/mdvw/config.toml`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    /// Key bindings by action name, e.g. `quit = ["q", "esc"]`.
    pub keys: BTreeMap<String, KeyList>,
    /// Problems found while loading, e.g. unknown keys (shown, not fatal).
    #[serde(skip)]
    pub warnings: Vec<String>,
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
            keys: BTreeMap::new(),
            warnings: Vec::new(),
        }
    }
}

/// One key or a list of keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum KeyList {
    One(String),
    Many(Vec<String>),
}

impl KeyList {
    pub fn as_slice(&self) -> &[String] {
        match self {
            KeyList::One(s) => std::slice::from_ref(s),
            KeyList::Many(v) => v,
        }
    }
}

/// `$XDG_CONFIG_HOME/mdvw` or `~/.config/mdvw` (APPDATA on Windows).
pub fn config_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        return dirs::config_dir().map(|d| d.join("mdvw"));
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .map(|d| d.join("mdvw"))
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
                Config::parse(&text).with_context(|| format!("invalid config {}", path.display()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", path.display())),
        }
    }
}

/// Top-level keys of the config file.
const KEYS: &[&str] = &[
    "theme",
    "max_width",
    "mouse",
    "watch",
    "hyperlinks",
    "toc",
    "remember_position",
    "images",
    "keys",
];

impl Config {
    /// Parses config text; unknown keys become warnings so a newer config
    /// still works with an older mdvw.
    pub fn parse(text: &str) -> Result<Config> {
        let table: toml::Table = toml::from_str(text)?;
        let mut cfg: Config = table.clone().try_into()?;
        cfg.warnings = table
            .keys()
            .filter(|k| !KEYS.contains(&k.as_str()))
            .map(|k| format!("unknown config key '{k}'"))
            .collect();
        Ok(cfg)
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
    fn warns_on_unknown_keys() {
        let c = Config::parse("colour = 1\nmouse = false\n").unwrap();
        assert_eq!(c.warnings, ["unknown config key 'colour'"]);
        assert!(!c.mouse);
        assert!(Config::parse("mouse = \"yes\"").is_err());
        let c = Config::parse("[keys]\nquit = \"x\"\ndown = [\"j\", \"ctrl-n\"]\n").unwrap();
        assert_eq!(c.keys["down"].as_slice(), ["j", "ctrl-n"]);
    }

    #[test]
    fn keys_list_is_complete() {
        let mut keys = BTreeMap::new();
        keys.insert("quit".into(), KeyList::One("q".into()));
        let cfg = Config {
            theme: Some("x".into()),
            keys,
            ..Default::default()
        };
        let table = toml::Table::try_from(cfg).unwrap();
        assert!(table.keys().all(|k| KEYS.contains(&k.as_str())));
        assert_eq!(table.len(), KEYS.len());
    }
}
