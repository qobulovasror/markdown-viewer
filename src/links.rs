//! Link target resolution and opening with the system handler.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// `#heading` within the current document.
    Anchor(String),
    /// Another document we can view, with optional anchor.
    Document(PathBuf, Option<String>),
    /// Anything to hand to the OS (web URL, image, PDF...).
    External(String),
}

/// File extensions mdv renders itself.
pub const VIEWABLE: &[&str] = &[
    "md", "markdown", "mdown", "mkd", "mdx", "txt", "rst", "adoc", "asciidoc", "org",
];

pub fn is_viewable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIEWABLE.contains(&e.to_ascii_lowercase().as_str()))
}

/// Resolves `url` relative to the directory of `base` (the current file).
pub fn resolve(url: &str, base: Option<&Path>) -> Target {
    if let Some(anchor) = url.strip_prefix('#') {
        return Target::Anchor(anchor.to_string());
    }
    if url.contains("://") || url.starts_with("mailto:") {
        if let Some(path) = url.strip_prefix("file://") {
            return local(path, None);
        }
        return Target::External(url.to_string());
    }
    let (path, anchor) = match url.split_once('#') {
        Some((p, a)) => (p, Some(a.to_string())),
        None => (url, None),
    };
    let path = percent_decode(path);
    let dir = base.and_then(Path::parent).unwrap_or(Path::new("."));
    let full = if Path::new(&path).is_absolute() {
        PathBuf::from(&path)
    } else {
        dir.join(&path)
    };
    local(&full.to_string_lossy(), anchor)
}

fn local(path: &str, anchor: Option<String>) -> Target {
    let p = PathBuf::from(path);
    if is_viewable(&p) {
        Target::Document(p, anchor)
    } else {
        Target::External(path.to_string())
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Opens a URL or path with the platform's default handler.
pub fn open_external(target: &str) -> std::io::Result<()> {
    let mut cmd = if cfg!(target_os = "macos") {
        Command::new("open")
    } else if cfg!(windows) {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]);
        c
    } else {
        Command::new("xdg-open")
    };
    cmd.arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_targets() {
        let base = Path::new("docs/README.md");
        assert_eq!(
            resolve("#usage", Some(base)),
            Target::Anchor("usage".into())
        );
        assert_eq!(
            resolve("guide/intro.md#setup", Some(base)),
            Target::Document("docs/guide/intro.md".into(), Some("setup".into()))
        );
        assert_eq!(
            resolve("https://x.dev", Some(base)),
            Target::External("https://x.dev".into())
        );
        // Compare as paths: on Windows `join` inserts `\`.
        let Target::External(img) = resolve("img/a%20b.png", Some(base)) else {
            panic!("expected external target");
        };
        assert_eq!(Path::new(&img), Path::new("docs/img/a b.png"));
    }
}
