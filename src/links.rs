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

/// File extensions mdvw renders itself.
pub const VIEWABLE: &[&str] = &[
    "md", "markdown", "mdown", "mkd", "mdx", "txt", "rst", "adoc", "asciidoc", "org",
];

pub fn is_viewable(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| VIEWABLE.contains(&e.to_ascii_lowercase().as_str()))
}

/// True for http(s) URLs and GitHub shorthands (`gh:owner/repo`, `github.com/...`).
pub fn is_remote(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    ["http://", "https://", "gh:", "github.com/"]
        .iter()
        .any(|p| lower.starts_with(p))
}

/// Resolves `url` relative to the directory of `base` (the current file).
pub fn resolve(url: &str, base: Option<&Path>) -> Target {
    if let Some(anchor) = url.strip_prefix('#') {
        return Target::Anchor(anchor.to_string());
    }
    if crate::render::ansi::is_external(url) {
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

/// Extensions that are safe to hand to the system viewer without asking.
const SAFE_EXT: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "pdf"];

/// True unless the target is a web/mail link or an existing local image/PDF;
/// anything else (scripts, apps, symlinks to them, custom schemes) could run code.
pub fn needs_confirm(target: &str) -> bool {
    let lower = target.to_ascii_lowercase();
    if ["http://", "https://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p))
    {
        return false;
    }
    if lower.contains("://") {
        return true;
    }
    // Check the real file: `shot.png` may be a symlink to an app or script.
    let Ok(real) = std::fs::canonicalize(target) else {
        return true;
    };
    let ext = real
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    !(real.is_file() && SAFE_EXT.contains(&ext.as_str()))
}

/// Opens a URL or path with the platform's default handler.
pub fn open_external(target: &str) -> std::io::Result<()> {
    let mut cmd = system_opener(target);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn system_opener(target: &str) -> Command {
    let mut c = Command::new("open");
    c.arg(target);
    c
}

/// Not `cmd /C start`, which interprets `&` and `|` in the URL. rundll32 takes
/// the rest of the command line verbatim, so the target is passed unquoted.
#[cfg(windows)]
fn system_opener(target: &str) -> Command {
    use std::os::windows::process::CommandExt;
    let mut c = Command::new("rundll32");
    c.raw_arg(format!("url.dll,FileProtocolHandler {target}"));
    c
}

#[cfg(not(any(windows, target_os = "macos")))]
fn system_opener(target: &str) -> Command {
    let mut c = Command::new("xdg-open");
    c.arg(target);
    c
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

    #[test]
    fn confirms_risky_targets() {
        assert!(!needs_confirm("https://x.dev/a.sh"));
        assert!(!needs_confirm("MAILTO:a@b.c"));
        assert!(needs_confirm("docs/img/missing.png"));
        assert!(needs_confirm("./run.command"));
        assert!(needs_confirm("tool.exe"));
        assert!(needs_confirm("vscode://file/x"));
        assert!(needs_confirm("docs/bin"));
        assert_eq!(
            resolve("MAILTO:a@b.c", None),
            Target::External("MAILTO:a@b.c".into())
        );
    }

    #[test]
    fn confirm_follows_symlinks() {
        let dir = std::env::temp_dir().join(format!("mdvw-links-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.PNG");
        let script = dir.join("run.command");
        std::fs::write(&png, b"x").unwrap();
        std::fs::write(&script, b"x").unwrap();
        assert!(!needs_confirm(png.to_str().unwrap()));
        #[cfg(unix)]
        {
            let link = dir.join("shot.png");
            std::os::unix::fs::symlink(&script, &link).unwrap();
            assert!(needs_confirm(link.to_str().unwrap()));
        }
        std::fs::remove_dir_all(dir).ok();
    }
}
