//! File change notifications for live reload.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};

use anyhow::{Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<()>,
}

impl FileWatcher {
    /// Watches the file's directory, so atomic saves (write + rename) are seen.
    pub fn new(path: &Path) -> Result<FileWatcher> {
        let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let name = target.file_name().map(|n| n.to_os_string());
        let dir: PathBuf = target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let (tx, rx) = channel();
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            if !matches!(ev.kind, EventKind::Modify(_) | EventKind::Create(_)) {
                return;
            }
            if ev
                .paths
                .iter()
                .any(|p| p.file_name().map(|n| n.to_os_string()) == name)
            {
                let _ = tx.send(());
            }
        })
        .context("cannot start file watcher")?;
        watcher
            .watch(&dir, RecursiveMode::NonRecursive)
            .with_context(|| format!("cannot watch {}", dir.display()))?;
        Ok(FileWatcher {
            _watcher: watcher,
            rx,
        })
    }

    /// Watches a folder tree for documents being added, removed or renamed.
    pub fn folder(root: &Path) -> Result<FileWatcher> {
        let (tx, rx) = channel();
        let base = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(ev) = res else { return };
            let structural = matches!(
                ev.kind,
                EventKind::Create(_)
                    | EventKind::Remove(_)
                    | EventKind::Modify(notify::event::ModifyKind::Name(_))
            );
            // Folders count too: a removed folder takes its documents along.
            // Only paths the file list could contain: not `.git`, `target`, ...
            let relevant = |p: &PathBuf| {
                let rel = p.strip_prefix(&base).unwrap_or(p);
                !crate::files::is_ignored(rel)
                    && (crate::links::is_viewable(p) || p.extension().is_none())
            };
            if structural && ev.paths.iter().any(relevant) {
                let _ = tx.send(());
            }
        })
        .context("cannot start folder watcher")?;
        watcher
            .watch(root, RecursiveMode::Recursive)
            .with_context(|| format!("cannot watch {}", root.display()))?;
        Ok(FileWatcher {
            _watcher: watcher,
            rx,
        })
    }

    /// True if the file changed since the last call.
    pub fn changed(&self) -> bool {
        let mut any = false;
        while self.rx.try_recv().is_ok() {
            any = true;
        }
        any
    }
}
