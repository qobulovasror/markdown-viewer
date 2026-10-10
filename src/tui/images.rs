//! Inline images via the Kitty, iTerm2 or Sixel graphics protocols.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

use super::app::Source;
use crate::layout::standalone_image;
use crate::parser::{Block, Document};

/// Tallest an inline image may get, in terminal rows.
const MAX_ROWS: u16 = 24;

struct Loaded {
    image: image::DynamicImage,
    protocol: StatefulProtocol,
}

pub struct Images {
    picker: Picker,
    /// Keyed by resolved location; `None` when it could not be loaded.
    cache: HashMap<Location, Option<Loaded>>,
    /// Download images from http(s) URLs.
    remote: bool,
    /// Downloads are only started before this time (set while planning).
    deadline: Option<Instant>,
}

/// Time one layout may spend downloading images; the rest wait for the next.
const DOWNLOAD_BUDGET: Duration = Duration::from_secs(8);

/// Where an image comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Location {
    File(PathBuf),
    Url(String),
}

impl Images {
    /// Queries the terminal; returns `None` without a real graphics protocol.
    pub fn detect(remote: bool) -> Option<Images> {
        // The terminal query can stall for seconds (e.g. inside tmux), so only
        // ask terminals known to support a graphics protocol.
        if !likely_supported() {
            return None;
        }
        let picker = Picker::from_query_stdio().ok()?;
        if picker.protocol_type() == ProtocolType::Halfblocks {
            return None;
        }
        Some(Images {
            picker,
            cache: HashMap::new(),
            remote,
            deadline: None,
        })
    }

    fn load(&mut self, loc: &Location) -> Option<&mut Loaded> {
        if !self.cache.contains_key(loc) {
            let image = match loc {
                Location::File(path) => image::ImageReader::open(path)
                    .ok()
                    .and_then(|r| r.with_guessed_format().ok())
                    .and_then(|r| r.decode().ok()),
                // Out of budget: not cached, so a later layout tries again.
                Location::Url(_) if self.deadline.is_none_or(|d| Instant::now() > d) => {
                    return None;
                }
                Location::Url(url) => download(url),
            };
            let loaded = image.map(|image| Loaded {
                protocol: self.picker.new_resize_protocol(image.clone()),
                image,
            });
            self.cache.insert(loc.clone(), loaded);
        }
        self.cache.get_mut(loc)?.as_mut()
    }

    /// Rows needed by each standalone local image at the given text width.
    pub fn plan(&mut self, doc: &Document, src: &Source, width: u16) -> HashMap<String, u16> {
        self.deadline = Some(Instant::now() + DOWNLOAD_BUDGET);
        let rows = self.plan_rows(doc, src, width);
        self.deadline = None;
        rows
    }

    fn plan_rows(&mut self, doc: &Document, src: &Source, width: u16) -> HashMap<String, u16> {
        let mut urls = Vec::new();
        collect(&doc.blocks, &mut urls);
        let font = self.picker.font_size();
        let mut out = HashMap::new();
        for url in urls {
            let Some(loc) = resolve(&url, src, self.remote) else {
                continue;
            };
            let Some(img) = self.load(&loc) else {
                continue;
            };
            let size = Resize::Fit(None).size_for(&img.image, font, Size::new(width, MAX_ROWS));
            if size.height > 0 {
                out.insert(url, size.height);
            }
        }
        out
    }

    pub fn protocol(&mut self, url: &str, src: &Source) -> Option<&mut StatefulProtocol> {
        let loc = resolve(url, src, self.remote)?;
        self.load(&loc).map(|l| &mut l.protocol)
    }
}

fn likely_supported() -> bool {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    if !var("TMUX").is_empty() {
        return false;
    }
    let term = var("TERM");
    !var("KITTY_WINDOW_ID").is_empty()
        || !var("WEZTERM_EXECUTABLE").is_empty()
        || ["kitty", "ghostty", "foot", "wezterm", "mlterm"]
            .iter()
            .any(|t| term.contains(t))
        || matches!(
            var("TERM_PROGRAM").as_str(),
            "iTerm.app" | "WezTerm" | "ghostty"
        )
}

/// Where to load an image from. Web images (and every image of a web
/// document) are only used when `remote` is on.
fn resolve(url: &str, src: &Source, remote: bool) -> Option<Location> {
    if let Some(base) = &src.url {
        return remote.then(|| join(base, url)).flatten().map(Location::Url);
    }
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return remote.then(|| Location::Url(url.to_string()));
    }
    if url.contains("://") && !lower.starts_with("file://") {
        return None;
    }
    match crate::links::resolve(url, src.path.as_deref()) {
        crate::links::Target::External(p) => Some(Location::File(PathBuf::from(p))),
        _ => None,
    }
}

#[cfg(feature = "remote")]
fn join(base: &str, url: &str) -> Option<String> {
    crate::remote::join(base, url)
}

#[cfg(not(feature = "remote"))]
fn join(_: &str, _: &str) -> Option<String> {
    None
}

#[cfg(feature = "remote")]
fn download(url: &str) -> Option<image::DynamicImage> {
    let bytes = crate::remote::fetch_bytes(url).ok()?;
    image::load_from_memory(&bytes).ok()
}

#[cfg(not(feature = "remote"))]
fn download(_: &str) -> Option<image::DynamicImage> {
    None
}

/// URLs of images that are alone in their paragraph.
fn collect(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Paragraph(inl) | Block::Plain(inl) => {
                if let Some(url) = standalone_image(inl) {
                    out.push(url.to_string());
                }
            }
            Block::BlockQuote { blocks, .. } | Block::FootnoteDef { blocks, .. } => {
                collect(blocks, out)
            }
            Block::List { items, .. } => items.iter().for_each(|i| collect(&i.blocks, out)),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_rows_for_standalone_local_images() {
        let dir = std::env::temp_dir().join(format!("mdvw-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        image::RgbImage::new(80, 64).save(&png).unwrap();
        let doc_path = dir.join("doc.md");

        #[allow(deprecated)]
        let picker = Picker::from_fontsize((8, 16).into());
        let mut images = Images {
            picker,
            cache: HashMap::new(),
            remote: false,
            deadline: None,
        };
        let doc = crate::parser::parse(
            "![a](a.png)\n\ntext ![inline](a.png) more\n\n![remote](https://x/y.png)\n",
            crate::parser::Format::Markdown,
        );
        let src = Source::from_file(&doc_path).unwrap_or(Source {
            path: Some(doc_path.clone()),
            url: None,
            text: String::new(),
            format: crate::parser::Format::Markdown,
        });
        let rows = images.plan(&doc, &src, 40);
        // 80x64 px at 8x16 px cells is 10x4 cells.
        assert_eq!(rows.get("a.png"), Some(&4));
        assert_eq!(rows.len(), 1);

        // A web document's images are skipped unless remote images are on.
        let web = Source {
            path: None,
            url: Some("https://h.dev/docs/x.md".into()),
            ..src
        };
        assert_eq!(resolve("a.png", &web, false), None);
        assert_eq!(
            resolve("a.png", &web, true),
            Some(Location::Url("https://h.dev/docs/a.png".into()))
        );
        assert_eq!(resolve("https://x/y.png", &web, false), None);
        std::fs::remove_dir_all(dir).ok();
    }
}
