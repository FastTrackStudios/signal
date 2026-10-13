use std::fs;
use std::path::{Path, PathBuf};

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use dawfile_reaper::{ReaperProject, parse_rpp_file};

fn candidate_fixtures() -> Vec<(String, PathBuf)> {
    let mut out = vec![(
        "tempo-map-advanced".to_string(),
        PathBuf::from("tests/fixtures/tempo-map-advanced.RPP"),
    )];

    let default_large = PathBuf::from("tests/fixtures/local/Goodness of God.RPP");
    if default_large.exists() {
        out.push(("goodness-of-god".to_string(), default_large));
    }

    if let Ok(env_path) = std::env::var("RPP_LARGE_FIXTURE") {
        let p = PathBuf::from(env_path);
        if p.exists() {
            out.push(("env-large".to_string(), p));
        }
    }

    out
}

fn parse_benchmark_data(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn bench_parse_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_only");
    group.sample_size(10);

    for (label, path) in candidate_fixtures() {
        if let Some(content) = parse_benchmark_data(&path) {
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_with_input(BenchmarkId::new("rpp", label), &content, |b, content| {
                b.iter(|| {
                    let parsed = parse_rpp_file(content).expect("parse failed");
                    std::hint::black_box(parsed.blocks.len());
                });
            });
        }
    }

    group.finish();
}

fn bench_parse_plus_typed(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_plus_typed");
    group.sample_size(10);

    for (label, path) in candidate_fixtures() {
        if let Some(content) = parse_benchmark_data(&path) {
            group.throughput(Throughput::Bytes(content.len() as u64));
            group.bench_with_input(BenchmarkId::new("rpp", label), &content, |b, content| {
                b.iter(|| {
                    let parsed = parse_rpp_file(content).expect("parse failed");
                    let typed =
                        ReaperProject::from_rpp_project(&parsed).expect("typed conversion failed");
                    std::hint::black_box((parsed.blocks.len(), typed.tracks.len()));
                });
            });
        }
    }

    group.finish();
}

criterion_group!(benches, bench_parse_only, bench_parse_plus_typed);
criterion_main!(benches);
