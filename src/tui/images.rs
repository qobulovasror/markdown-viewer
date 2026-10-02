//! Inline images via the Kitty, iTerm2 or Sixel graphics protocols.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ratatui::layout::Size;
use ratatui_image::Resize;
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;

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
    /// Keyed by resolved path; `None` when the file could not be decoded.
    cache: HashMap<PathBuf, Option<Loaded>>,
}

impl Images {
    /// Queries the terminal; returns `None` without a real graphics protocol.
    pub fn detect() -> Option<Images> {
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
        })
    }

    fn load(&mut self, path: &Path) -> Option<&mut Loaded> {
        if !self.cache.contains_key(path) {
            let loaded = image::ImageReader::open(path)
                .ok()
                .and_then(|r| r.with_guessed_format().ok())
                .and_then(|r| r.decode().ok())
                .map(|image| Loaded {
                    protocol: self.picker.new_resize_protocol(image.clone()),
                    image,
                });
            self.cache.insert(path.to_path_buf(), loaded);
        }
        self.cache.get_mut(path)?.as_mut()
    }

    /// Rows needed by each standalone local image at the given text width.
    pub fn plan(&mut self, doc: &Document, base: Option<&Path>, width: u16) -> HashMap<String, u16> {
        let mut urls = Vec::new();
        collect(&doc.blocks, &mut urls);
        let font = self.picker.font_size();
        let mut out = HashMap::new();
        for url in urls {
            let Some(path) = resolve(&url, base) else { continue };
            let Some(img) = self.load(&path) else { continue };
            let size = Resize::Fit(None).size_for(&img.image, font, Size::new(width, MAX_ROWS));
            if size.height > 0 {
                out.insert(url, size.height);
            }
        }
        out
    }

    pub fn protocol(&mut self, url: &str, base: Option<&Path>) -> Option<&mut StatefulProtocol> {
        let path = resolve(url, base)?;
        self.load(&path).map(|l| &mut l.protocol)
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
        || matches!(var("TERM_PROGRAM").as_str(), "iTerm.app" | "WezTerm" | "ghostty")
}

/// Local file path for an image URL; remote images are not fetched.
fn resolve(url: &str, base: Option<&Path>) -> Option<PathBuf> {
    if url.contains("://") && !url.starts_with("file://") {
        return None;
    }
    match crate::links::resolve(url, base) {
        crate::links::Target::External(p) => Some(PathBuf::from(p)),
        _ => None,
    }
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
            Block::BlockQuote { blocks, .. } | Block::FootnoteDef { blocks, .. } => collect(blocks, out),
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
        let dir = std::env::temp_dir().join(format!("mdv-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let png = dir.join("a.png");
        image::RgbImage::new(80, 64).save(&png).unwrap();
        let doc_path = dir.join("doc.md");

        let mut images = Images {
            picker: Picker::from_fontsize((8, 16).into()),
            cache: HashMap::new(),
        };
        let doc = crate::parser::parse(
            "![a](a.png)\n\ntext ![inline](a.png) more\n\n![remote](https://x/y.png)\n",
            crate::parser::Format::Markdown,
        );
        let rows = images.plan(&doc, Some(&doc_path), 40);
        // 80x64 px at 8x16 px cells is 10x4 cells.
        assert_eq!(rows.get("a.png"), Some(&4));
        assert_eq!(rows.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }
}
