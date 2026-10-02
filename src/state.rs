//! Persistent per-file state: last reading position and bookmarks.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A position that survives re-wrapping: heading anchor plus line offset.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub heading: Option<String>,
    pub offset: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileState {
    #[serde(default)]
    pub position: Position,
    #[serde(default)]
    pub bookmarks: BTreeMap<char, Position>,
    /// Unix seconds of last access, used to prune old entries.
    #[serde(default)]
    pub seen: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    #[serde(default)]
    files: HashMap<PathBuf, FileState>,
}

const MAX_FILES: usize = 500;

fn state_path() -> Option<PathBuf> {
    let dir = if cfg!(windows) {
        dirs::data_local_dir()
    } else {
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/state")))
    };
    dir.map(|d| d.join("mdvw").join("state.json"))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn key(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

impl State {
    /// Loads saved state; any error yields an empty state.
    pub fn load() -> State {
        state_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&mut self) -> std::io::Result<()> {
        let Some(path) = state_path() else {
            return Ok(());
        };
        if self.files.len() > MAX_FILES {
            let mut seen: Vec<u64> = self.files.values().map(|f| f.seen).collect();
            seen.sort_unstable_by(|a, b| b.cmp(a));
            let cutoff = seen[MAX_FILES - 1];
            self.files.retain(|_, f| f.seen >= cutoff);
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string(self).map_err(std::io::Error::other)?;
        // Write then rename so a crash never leaves a truncated file.
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(tmp, path)
    }

    pub fn get(&self, path: &Path) -> Option<&FileState> {
        self.files.get(&key(path))
    }

    pub fn entry(&mut self, path: &Path) -> &mut FileState {
        let f = self.files.entry(key(path)).or_default();
        f.seen = now();
        f
    }
}
