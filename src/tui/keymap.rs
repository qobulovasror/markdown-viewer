//! Viewer actions and their key bindings (overridable via `[keys]` in the config).

use std::collections::{BTreeMap, HashMap};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::config::KeyList;

macro_rules! actions {
    ($($variant:ident $name:literal [$($key:literal),*] $desc:literal,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Action {
            $($variant,)*
        }

        impl Action {
            pub const ALL: &[Action] = &[$(Action::$variant,)*];

            /// Name used in the `[keys]` config section.
            pub fn name(self) -> &'static str {
                match self {
                    $(Action::$variant => $name,)*
                }
            }

            pub fn description(self) -> &'static str {
                match self {
                    $(Action::$variant => $desc,)*
                }
            }

            fn default_keys(self) -> &'static [&'static str] {
                match self {
                    $(Action::$variant => &[$($key),*],)*
                }
            }
        }
    };
}

actions! {
    Down "down" ["j", "down"] "Scroll down (count: lines)",
    Up "up" ["k", "up"] "Scroll up",
    HalfDown "half_down" ["d", "ctrl-d"] "Half page down",
    HalfUp "half_up" ["u", "ctrl-u"] "Half page up",
    PageDown "page_down" ["space", "f", "ctrl-f", "pagedown"] "Page down",
    PageUp "page_up" ["b", "ctrl-b", "pageup"] "Page up",
    Top "top" ["g", "home"] "Top (count: go to line)",
    Bottom "bottom" ["G", "end"] "Bottom (count: go to line)",
    Percent "percent" ["%"] "Go to count percent, e.g. 50%",
    NextHeading "next_heading" ["]"] "Next heading",
    PrevHeading "prev_heading" ["["] "Previous heading",
    Toc "toc" ["t"] "Table of contents (/ filters, h/l switch focus)",
    Files "files" ["F"] "File panel",
    Finder "finder" ["o", "ctrl-p"] "Fuzzy find and open a file",
    Recent "recent" ["R"] "Recently opened files",
    NextFile "next_file" ["}"] "Next file in the list / folder",
    PrevFile "prev_file" ["{"] "Previous file in the list / folder",
    FocusPanel "focus_panel" ["h"] "Focus the side panel",
    ScrollLeft "scroll_left" ["left", "<"] "Scroll long code lines left",
    ScrollRight "scroll_right" ["right", ">"] "Scroll long code lines right",
    Wrap "wrap" ["W"] "Toggle wrapping of long code lines",
    Search "search" ["/"] "Search (regex, smart-case)",
    Grep "grep" ["S"] "Search all files in the list / folder",
    NextMatch "next_match" ["n"] "Next match",
    PrevMatch "prev_match" ["N"] "Previous match",
    NextLink "next_link" ["tab"] "Next link",
    PrevLink "prev_link" ["shift-tab"] "Previous link",
    Follow "follow" ["enter"] "Open focused link",
    OpenExternal "open_external" ["O"] "Open focused link in the browser / system app",
    Back "back" ["backspace", "ctrl-o", "H"] "Go back",
    Forward "forward" ["L"] "Go forward",
    Mark "mark" ["m"] "Set bookmark: m + letter",
    JumpMark "jump_mark" ["'"] "Jump to bookmark: ' + letter",
    CopyCode "copy_code" ["y"] "Copy code block: y + 1-9, yy = on screen",
    CopyLink "copy_link" ["Y"] "Copy focused link, or a link to this section",
    Edit "edit" ["e"] "Edit file in $EDITOR",
    Reload "reload" ["r"] "Reload file",
    Watch "watch" ["w"] "Watch file for changes",
    FocusMode "focus_mode" ["z"] "Toggle focus mode / full width",
    Theme "theme" ["T"] "Cycle color theme",
    Mouse "mouse" ["M"] "Toggle mouse capture (text selection)",
    Info "info" ["i"] "Document info and front matter",
    Help "help" ["?"] "Toggle this help",
    Cancel "cancel" ["esc"] "Clear search / link focus / count",
    Quit "quit" ["q", "ctrl-c"] "Quit",
}

/// A key with only the modifiers that matter for bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    code: KeyCode,
    mods: KeyModifiers,
}

impl Key {
    pub fn from_event(k: &KeyEvent) -> Key {
        // Shift is already part of the character (`G`) or the code (BackTab).
        let mods = k.modifiers & (KeyModifiers::CONTROL | KeyModifiers::ALT);
        let code = match k.code {
            KeyCode::Char(c) if mods.contains(KeyModifiers::CONTROL) => {
                KeyCode::Char(c.to_ascii_lowercase())
            }
            c => c,
        };
        Key { code, mods }
    }

    /// Parses `j`, `G`, `ctrl-d`, `alt-x`, `shift-tab`, `space`, `pagedown`, `f5`...
    pub fn parse(spec: &str) -> Option<Key> {
        let mut mods = KeyModifiers::NONE;
        let mut shift = false;
        let mut rest = spec;
        loop {
            let lower = rest.to_ascii_lowercase();
            let (m, len) = if lower.starts_with("ctrl-") {
                (KeyModifiers::CONTROL, 5)
            } else if lower.starts_with("alt-") {
                (KeyModifiers::ALT, 4)
            } else if lower.starts_with("shift-") {
                shift = true;
                (KeyModifiers::NONE, 6)
            } else {
                break;
            };
            if rest.len() == len {
                break; // a lone `-` key after the prefix is not supported
            }
            mods |= m;
            rest = &rest[len..];
        }
        let lower = rest.to_ascii_lowercase();
        let code = match lower.as_str() {
            "esc" | "escape" => KeyCode::Esc,
            "enter" | "return" => KeyCode::Enter,
            "tab" if shift => KeyCode::BackTab,
            // Shift only combines with tab and letters.
            l if shift && !(l.len() == 1 && l.chars().all(|c| c.is_ascii_alphabetic())) => {
                return None;
            }
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            "space" => KeyCode::Char(' '),
            "backspace" => KeyCode::Backspace,
            "delete" | "del" => KeyCode::Delete,
            "insert" => KeyCode::Insert,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            f if f.len() > 1 && f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
                KeyCode::F(f[1..].parse().ok()?)
            }
            _ => {
                let mut chars = rest.chars();
                let c = chars.next()?;
                if chars.next().is_some() {
                    return None;
                }
                let c = if mods.contains(KeyModifiers::CONTROL) {
                    c.to_ascii_lowercase()
                } else if shift {
                    c.to_ascii_uppercase()
                } else {
                    c
                };
                KeyCode::Char(c)
            }
        };
        Some(Key { code, mods })
    }

    /// Short label for the help screen.
    pub fn label(&self) -> String {
        let name = match self.code {
            KeyCode::Char(' ') => "Space".into(),
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Esc => "Esc".into(),
            KeyCode::Enter => "Enter".into(),
            KeyCode::Tab => "Tab".into(),
            KeyCode::BackTab => "Shift-Tab".into(),
            KeyCode::Backspace => "Backspace".into(),
            KeyCode::Delete => "Del".into(),
            KeyCode::Insert => "Ins".into(),
            KeyCode::Up => "↑".into(),
            KeyCode::Down => "↓".into(),
            KeyCode::Left => "←".into(),
            KeyCode::Right => "→".into(),
            KeyCode::Home => "Home".into(),
            KeyCode::End => "End".into(),
            KeyCode::PageUp => "PgUp".into(),
            KeyCode::PageDown => "PgDn".into(),
            KeyCode::F(n) => format!("F{n}"),
            other => format!("{other:?}"),
        };
        let mut out = String::new();
        if self.mods.contains(KeyModifiers::CONTROL) {
            out.push_str("Ctrl-");
        }
        if self.mods.contains(KeyModifiers::ALT) {
            out.push_str("Alt-");
        }
        out + &name
    }
}

pub struct Keymap {
    bindings: HashMap<Key, Action>,
    /// Keys per action in display order.
    keys: BTreeMap<Action, Vec<Key>>,
}

impl Default for Keymap {
    fn default() -> Self {
        Keymap::new(&BTreeMap::new()).0
    }
}

impl Keymap {
    /// Defaults with `overrides` (action name → keys) applied; returns warnings.
    pub fn new(overrides: &BTreeMap<String, KeyList>) -> (Keymap, Vec<String>) {
        let mut warnings = Vec::new();
        let mut keys: BTreeMap<Action, Vec<Key>> = Action::ALL
            .iter()
            .map(|&a| {
                let ks = a.default_keys().iter().filter_map(|s| Key::parse(s));
                (a, ks.collect())
            })
            .collect();
        let mut user = Vec::new();
        for (name, list) in overrides {
            let Some(&action) = Action::ALL.iter().find(|a| a.name() == name) else {
                warnings.push(format!("unknown action '{name}' in [keys]"));
                continue;
            };
            let mut ks = Vec::new();
            for spec in list.as_slice() {
                match Key::parse(spec) {
                    Some(k) => ks.push(k),
                    None => warnings.push(format!("invalid key '{spec}' for '{name}'")),
                }
            }
            user.extend(ks.iter().map(|&k| (k, action)));
            keys.insert(action, ks);
        }
        // User keys win over defaults. A key given to two actions in the config
        // stays with the first (by action name) and is reported.
        let user_actions: Vec<Action> = user.iter().map(|&(_, a)| a).collect();
        for (k, a) in &user {
            if !keys[a].contains(k) {
                continue;
            }
            for (other, ks) in keys.iter_mut() {
                if other != a && ks.contains(k) {
                    ks.retain(|x| x != k);
                    if user_actions.contains(other) {
                        warnings.push(format!(
                            "key '{}' is bound to both '{}' and '{}'",
                            k.label(),
                            a.name(),
                            other.name()
                        ));
                    }
                }
            }
        }
        let bindings = keys
            .iter()
            .flat_map(|(&a, ks)| ks.iter().map(move |&k| (k, a)))
            .collect();
        (Keymap { bindings, keys }, warnings)
    }

    pub fn action(&self, key: &KeyEvent) -> Option<Action> {
        self.bindings.get(&Key::from_event(key)).copied()
    }

    pub fn is_bound(&self, key: &KeyEvent) -> bool {
        self.action(key).is_some()
    }

    /// `j, ↓` style label of an action's keys.
    pub fn label(&self, action: Action) -> String {
        let ks = self.keys.get(&action).map(Vec::as_slice).unwrap_or(&[]);
        if ks.is_empty() {
            return "(unbound)".into();
        }
        ks.iter().map(Key::label).collect::<Vec<_>>().join(", ")
    }

    /// (keys, description) rows for the help screen and man page.
    pub fn help(&self) -> Vec<(String, &'static str)> {
        Action::ALL
            .iter()
            .map(|&a| (self.label(a), a.description()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn parses_specs() {
        let k = |s| Key::parse(s).unwrap();
        assert_eq!(
            k("ctrl-d"),
            Key::from_event(&ev(KeyCode::Char('d'), KeyModifiers::CONTROL))
        );
        assert_eq!(
            k("G"),
            Key::from_event(&ev(KeyCode::Char('G'), KeyModifiers::SHIFT))
        );
        assert_eq!(k("shift-g"), k("G"));
        assert_eq!(
            k("shift-tab"),
            Key::from_event(&ev(KeyCode::BackTab, KeyModifiers::SHIFT))
        );
        assert_eq!(k("PgDn").code, KeyCode::PageDown);
        assert_eq!(k("f5").code, KeyCode::F(5));
        assert_eq!(k("f").code, KeyCode::Char('f'));
        assert_eq!(k("-").code, KeyCode::Char('-'));
        assert!(Key::parse("ctrl-").is_none());
        assert_eq!(k("ctrl--").code, KeyCode::Char('-'));
        assert!(Key::parse("jk").is_none());
        assert!(Key::parse("").is_none());
    }

    #[test]
    fn defaults_and_overrides() {
        let km = Keymap::default();
        let j = ev(KeyCode::Char('j'), KeyModifiers::NONE);
        let q = ev(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(km.action(&j), Some(Action::Down));

        let mut o = BTreeMap::new();
        o.insert("quit".to_string(), KeyList::One("j".into()));
        o.insert("nope".to_string(), KeyList::One("x".into()));
        o.insert(
            "down".to_string(),
            KeyList::Many(vec!["ctrl-n".into(), "??".into()]),
        );
        let (km, warnings) = Keymap::new(&o);
        assert_eq!(km.action(&j), Some(Action::Quit));
        assert_eq!(km.action(&q), None);
        assert_eq!(
            km.action(&ev(KeyCode::Char('n'), KeyModifiers::CONTROL)),
            Some(Action::Down)
        );
        assert_eq!(warnings.len(), 2);
        assert_eq!(km.label(Action::Down), "Ctrl-n");

        let mut o = BTreeMap::new();
        o.insert("down".to_string(), KeyList::One("x".into()));
        o.insert("quit".to_string(), KeyList::One("x".into()));
        let (km, warnings) = Keymap::new(&o);
        let x = ev(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(km.action(&x), Some(Action::Down));
        assert_eq!(warnings, ["key 'x' is bound to both 'down' and 'quit'"]);
        assert!(Key::parse("shift-up").is_none() && Key::parse("shift-1").is_none());
    }

    #[test]
    fn default_keys_parse_and_do_not_collide() {
        let mut seen = HashMap::new();
        for &a in Action::ALL {
            for s in a.default_keys() {
                let k = Key::parse(s).unwrap_or_else(|| panic!("bad default key {s}"));
                assert!(seen.insert(k, a).is_none(), "{s} bound twice");
            }
        }
    }
}
