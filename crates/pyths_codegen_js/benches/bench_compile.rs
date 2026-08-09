use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::path::Path;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fixtures_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(fixtures_dir().join(name)).unwrap()
}

struct Input {
    name: &'static str,
    source: String,
}

fn inputs() -> Vec<Input> {
    vec![
        Input {
            name: "tiny",
            source: r#"print("hello")"#.to_string(),
        },
        Input {
            name: "small",
            source: load_fixture("dataclass.ps"),
        },
        Input {
            name: "medium",
            source: load_fixture("functions.ps"),
        },
        Input {
            name: "large",
            source: load_fixture("stress_many_functions.ps"),
        },
        Input {
            name: "stress",
            source: load_fixture("stress_large_class.ps"),
        },
    ]
}

// ── Benchmark groups ─────────────────────────────────────────────────────────

fn bench_lexer(c: &mut Criterion) {
    let mut group = c.benchmark_group("lexer");
    for input in &inputs() {
        group.throughput(Throughput::Bytes(input.source.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("lex", input.name),
            &input.source,
            |b, src| {
                b.iter(|| pyths_lexer::lex(src).unwrap());
            },
        );
    }
    group.finish();
}

fn bench_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");
    for input in &inputs() {
        group.throughput(Throughput::Bytes(input.source.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("parse", input.name),
            &input.source,
            |b, src| {
                b.iter(|| pyths_parser::parse(src).unwrap());
            },
        );
    }
    group.finish();
}

fn bench_codegen(c: &mut Criterion) {
    let mut group = c.benchmark_group("codegen");
    let all_inputs = inputs();
    // Pre-parse all inputs so we only measure codegen
    let parsed: Vec<_> = all_inputs
        .iter()
        .map(|input| {
            let module = pyths_parser::parse(&input.source).unwrap();
            (input.name, input.source.len(), module)
        })
        .collect();

    for (name, size, module) in &parsed {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::new("codegen", name), module, |b, module| {
            b.iter(|| pyths_codegen_js::codegen(module));
        });
    }
    group.finish();
}

fn bench_end_to_end(c: &mut Criterion) {
    let mut group = c.benchmark_group("end_to_end");
    for input in &inputs() {
        group.throughput(Throughput::Bytes(input.source.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("compile", input.name),
            &input.source,
            |b, src| {
                b.iter(|| {
                    let module = pyths_parser::parse(src).unwrap();
                    pyths_codegen_js::codegen(&module)
                });
            },
        );
    }
    group.finish();
}

fn bench_sourcemap(c: &mut Criterion) {
    let mut group = c.benchmark_group("sourcemap");
    for input in &inputs() {
        group.throughput(Throughput::Bytes(input.source.len() as u64));
        group.bench_with_input(
            BenchmarkId::new("with_sourcemap", input.name),
            &input.source,
            |b, src| {
                b.iter(|| {
                    let module = pyths_parser::parse(src).unwrap();
                    pyths_codegen_js::codegen_with_sourcemap(
                        &module,
                        src,
                        "bench.ps",
                        "bench.js",
                        &std::collections::HashMap::new(),
                    )
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_lexer,
    bench_parser,
    bench_codegen,
    bench_end_to_end,
    bench_sourcemap
);
criterion_main!(benches);
