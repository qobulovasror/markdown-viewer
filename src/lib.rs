//! mdvw: terminal viewer for Markdown and similar documents.
//!
//! Pipeline: `parser` (text → AST) → `layout` (AST → styled lines)
//! → `render::ansi` (stdout) or `tui` (interactive viewer).

pub mod clipboard;
pub mod color;
pub mod config;
pub mod files;
pub mod highlight;
pub mod layout;
pub mod links;
pub mod math;
pub mod mermaid;
pub mod parser;
pub mod render;
pub mod search;
pub mod state;
pub mod theme;
pub mod tui;
pub mod watch;

use color::ColorDepth;
use parser::Format;
use theme::Theme;

/// Options for rendering a document as text.
pub struct PrintOptions<'a> {
    pub width: usize,
    pub theme: &'a Theme,
    pub depth: ColorDepth,
    pub hyperlinks: bool,
}

/// Parses and renders `src` to a string (ANSI-styled unless `depth` is `None`).
pub fn render_to_string(src: &str, format: Format, opts: &PrintOptions) -> String {
    let doc = parser::parse(src, format);
    let rendered = layout::layout(
        &doc,
        opts.theme,
        &layout::Options {
            width: opts.width,
            code_numbers: false,
            front_matter: true,
            image_rows: Default::default(),
        },
    );
    render::ansi::render(
        &rendered,
        &render::ansi::AnsiOptions {
            depth: opts.depth,
            hyperlinks: opts.hyperlinks,
        },
    )
}
