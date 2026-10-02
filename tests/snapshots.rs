//! Rendering snapshots of the example documents (plain and ANSI).

use mdvw::color::ColorDepth;
use mdvw::parser::Format;
use mdvw::theme::Theme;
use mdvw::{PrintOptions, render_to_string};

fn render(file: &str, width: usize, depth: ColorDepth) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("examples")
        .join(file);
    let src = std::fs::read_to_string(&path).unwrap();
    let theme = Theme::by_name("dark").unwrap();
    render_to_string(
        &src,
        Format::from_path(&path),
        &PrintOptions {
            width,
            theme: &theme,
            depth,
            hyperlinks: true,
        },
    )
}

#[test]
fn markdown_plain() {
    insta::assert_snapshot!(render("sample.md", 72, ColorDepth::None));
}

#[test]
fn markdown_narrow() {
    insta::assert_snapshot!(render("sample.md", 32, ColorDepth::None));
}

#[test]
fn markdown_ansi() {
    insta::assert_snapshot!(render("sample.md", 72, ColorDepth::Ansi256));
}

#[test]
fn org_plain() {
    insta::assert_snapshot!(render("sample.org", 60, ColorDepth::None));
}

#[test]
fn asciidoc_plain() {
    insta::assert_snapshot!(render("sample.adoc", 60, ColorDepth::None));
}

#[test]
fn rst_plain() {
    insta::assert_snapshot!(render("sample.rst", 60, ColorDepth::None));
}
