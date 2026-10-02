//! Randomized smoke test: parsers and layout must never panic.

use super::{Format, parse};
use crate::layout::{Options, layout};
use crate::theme::Theme;

const PIECES: &[&str] = &[
    "\n", "\n\n", " ", "  ", "\t", "#", "##", "*", "**", "_", "__", "`", "``", "```", "~~~", "=",
    "==", "====", "-", "----", "+", ".", "..", ":", "::", "|", "|===", "[", "]", "[[", "]]", "(",
    ")", "<", ">", "<<", ">>", "<a href=\"x\">", "</a>", "<img src=y alt=\"é\">", "<br>", "<!--",
    "-->", "$", "$$", "\\frac{", "}", "{", "^", "_{", "\\alpha", ":rocket:", "#+BEGIN_SRC rs",
    "#+END_SRC", "#+TITLE:", ".. note::", ".. code-block:: py", ".. _x: http://y", "`a <b>`_",
    "1.", "1)", "- [ ]", "* [x]", "[!NOTE]", "> ", "é", "日本", "🚀", "ä", "x", "word", "http://a.b",
    "link:u[t]", "image::p.png[alt]", "[source,rust]", "NOTE:", "[^1]", "[^1]:", "\u{feff}",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 as usize
    }
}

#[test]
fn random_inputs_do_not_panic() {
    let theme = Theme::default_dark();
    // MDV_FUZZ_ITERS / MDV_FUZZ_SEED allow longer local runs.
    let env = |k: &str, d: u64| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
    let mut rng = Rng(env("MDV_FUZZ_SEED", 0x9E37_79B9_7F4A_7C15) | 1);
    for _ in 0..env("MDV_FUZZ_ITERS", 3000) {
        let len = rng.next() % 60;
        let src: String = (0..len).map(|_| PIECES[rng.next() % PIECES.len()]).collect();
        for format in [Format::Markdown, Format::Org, Format::AsciiDoc, Format::Rst, Format::Text] {
            let doc = parse(&src, format);
            for width in [1, 7, 40] {
                let opts = Options {
                    width,
                    code_numbers: true,
                    front_matter: true,
                };
                let r = std::panic::catch_unwind(|| layout(&doc, &theme, &opts));
                assert!(r.is_ok(), "layout panic: format {format:?} width {width} input {src:?}");
            }
        }
    }
}
