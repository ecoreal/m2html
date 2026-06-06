use criterion::{Criterion, criterion_group, criterion_main};
use m2html_core::{PageStyle, RenderOptions, render};

fn sample_markdown() -> String {
    let mut markdown = String::new();

    for section in 0..200 {
        markdown.push_str(&format!("# Section {section}\n\n"));
        markdown.push_str(
            "This is a paragraph with **bold text**, [a link](https://example.com), and `code`.\n\n",
        );
        markdown.push_str("| name | value |\n| --- | --- |\n| alpha | beta |\n\n");
        markdown.push_str("```rust\nfn main() {\n    println!(\"hello\");\n}\n```\n\n");
    }

    markdown
}

fn bench_fragment(c: &mut Criterion) {
    let markdown = sample_markdown();
    let options = RenderOptions::default();

    c.bench_function("fragment_safe", |b| {
        b.iter(|| render(&markdown, &options).expect("render should succeed"))
    });
}

fn bench_page_highlighted(c: &mut Criterion) {
    let markdown = sample_markdown();
    let options = RenderOptions {
        page: true,
        highlight: true,
        style: PageStyle::Github,
        title: Some("Benchmark".to_owned()),
        ..RenderOptions::default()
    };

    c.bench_function("page_highlighted", |b| {
        b.iter(|| render(&markdown, &options).expect("render should succeed"))
    });
}

criterion_group!(benches, bench_fragment, bench_page_highlighted);
criterion_main!(benches);
