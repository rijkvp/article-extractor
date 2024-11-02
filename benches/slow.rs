use article_extractor::FullTextParser;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use url::Url;

pub fn bench(c: &mut Criterion) {
    let html = include_str!("../resources/tests/slow.html").to_string();
    c.bench_function("parse", |b| b.iter(|| parse(black_box(html.clone()))));
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench
}
criterion_main!(benches);

fn parse(html: String) {
    let parser = FullTextParser::new(None);
    parser
        .parse_offline(
            vec![html.to_string()],
            None,
            Some(Url::parse("https://spectrum.ieee.org/the-off-the-shelf-stellarator").unwrap()),
        )
        .unwrap();
}
