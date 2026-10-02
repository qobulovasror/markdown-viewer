use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

use mdv::layout::{Options, layout};
use mdv::parser::{Format, parse};
use mdv::theme::Theme;

fn big_document() -> String {
    let sample = include_str!("../examples/sample.md");
    // ~1 MB: front matter only once, then the body repeated.
    let body = sample.splitn(3, "---").nth(2).unwrap_or(sample);
    body.repeat(1_000_000 / body.len() + 1)
}

fn bench(c: &mut Criterion) {
    let src = big_document();
    let theme = Theme::by_name("dark").unwrap();
    let opts = Options {
        width: 100,
        code_numbers: true,
        front_matter: true,
        image_rows: Default::default(),
    };
    c.bench_function("parse 1MB markdown", |b| {
        b.iter(|| parse(black_box(&src), Format::Markdown))
    });
    let doc = parse(&src, Format::Markdown);
    c.bench_function("layout 1MB markdown", |b| {
        b.iter(|| layout(black_box(&doc), &theme, &opts))
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench
}
criterion_main!(benches);
